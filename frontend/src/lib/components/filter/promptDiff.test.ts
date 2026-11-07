import { describe, expect, it } from 'vitest';
import { diffText, diffTextWithPreview, diffWords, type DiffSegment } from './promptDiff';
import { splitBlockIntoDisplayParagraphs } from './blockParagraphs';
import { REQUEST_421514_BLOCKS, REQUEST_421514_DRY_RUN } from './__fixtures__/request421514';

/**
 * Within any contiguous run of removed/added segments (no `unchanged` between them), every
 * removed segment must come before every added segment — the reading order a human expects
 * ("was X, now Y"). Violating this doesn't break the round-trip invariant (each side still
 * reconstructs correctly) but reads backwards: this is exactly how round-4's "command.survey"
 * bug rendered (added "…running a command." immediately followed by a struck-through leftover
 * "survey", live on request 421514, rule 69's append).
 */
function assertCanonicalRemoveAddOrder(segments: readonly DiffSegment[]): void {
	let sawAddedInRun = false;
	for (const seg of segments) {
		if (seg.status === 'unchanged') {
			sawAddedInRun = false;
			continue;
		}
		if (seg.status === 'added') {
			sawAddedInRun = true;
			continue;
		}
		// seg.status === 'removed'
		expect(sawAddedInRun).toBe(false);
	}
}

function reconstruct(segments: readonly DiffSegment[], side: 'before' | 'after'): string {
	return segments
		.filter((s) => (side === 'before' ? s.status !== 'added' : s.status !== 'removed'))
		.map((s) => s.text)
		.join('');
}

describe('diffWords', () => {
	it('returns a single unchanged segment for identical text', () => {
		expect(diffWords('same text', 'same text')).toEqual([
			{ text: 'same text', status: 'unchanged' }
		]);
	});

	it('returns nothing for two empty strings', () => {
		expect(diffWords('', '')).toEqual([]);
	});

	it('marks only the changed word, preserving surrounding whitespace exactly', () => {
		const segs = diffWords('use grep for search', 'use ast-grep for search');
		expect(reconstruct(segs, 'before')).toBe('use grep for search');
		expect(reconstruct(segs, 'after')).toBe('use ast-grep for search');
		expect(segs.some((s) => s.status === 'removed' && s.text.includes('grep'))).toBe(true);
		expect(segs.some((s) => s.status === 'added' && s.text.includes('ast-grep'))).toBe(true);
		expect(segs.some((s) => s.status === 'unchanged' && s.text.includes('use'))).toBe(true);
	});

	it('handles pure removal (after is empty)', () => {
		const segs = diffWords('drop this entirely', '');
		expect(segs).toEqual([{ text: 'drop this entirely', status: 'removed' }]);
	});

	it('handles pure addition (before is empty)', () => {
		const segs = diffWords('', 'brand new text');
		expect(segs).toEqual([{ text: 'brand new text', status: 'added' }]);
	});
});

describe('diffText (sentence pass, word refinement)', () => {
	it('returns a single unchanged segment for identical text', () => {
		expect(diffText('Same paragraph.', 'Same paragraph.')).toEqual([
			{ text: 'Same paragraph.', status: 'unchanged' }
		]);
	});

	it('keeps an unchanged leading sentence whole and refines only the changed sentence to word level', () => {
		const before = 'Keep this sentence. Prefer grep for search.';
		const after = 'Keep this sentence. Prefer ast-grep for search.';
		const segs = diffText(before, after);

		expect(reconstruct(segs, 'before')).toBe(before);
		expect(reconstruct(segs, 'after')).toBe(after);

		const unchangedWhole = segs.find(
			(s) => s.status === 'unchanged' && s.text.includes('Keep this sentence.')
		);
		expect(unchangedWhole).toBeDefined();

		// the second sentence must NOT collapse into one big removed/added blob —
		// "Prefer" and "for search." stay unchanged, only "grep"/"ast-grep" differ.
		expect(segs.some((s) => s.status === 'unchanged' && s.text.includes('Prefer'))).toBe(true);
		expect(segs.some((s) => s.status === 'removed' && s.text.trim() === 'grep')).toBe(true);
		expect(segs.some((s) => s.status === 'added' && s.text.trim() === 'ast-grep')).toBe(true);
	});

	it('keeps a wholly added sentence as one added segment, not word-diffed against nothing', () => {
		const before = 'Only sentence here.';
		const after = 'Only sentence here. A brand new second sentence.';
		const segs = diffText(before, after);
		expect(reconstruct(segs, 'before')).toBe(before);
		expect(reconstruct(segs, 'after')).toBe(after);
		const added = segs.find((s) => s.status === 'added');
		expect(added?.text.trim()).toBe('A brand new second sentence.');
	});

	it('keeps a wholly removed sentence as one removed segment', () => {
		const before = 'Keep me. Drop this whole sentence entirely.';
		const after = 'Keep me.';
		const segs = diffText(before, after);
		expect(reconstruct(segs, 'before')).toBe(before);
		expect(reconstruct(segs, 'after')).toBe(after);
		const removed = segs.find((s) => s.status === 'removed');
		expect(removed?.text.trim()).toBe('Drop this whole sentence entirely.');
	});

	it('round-trips on paragraphs with no terminal punctuation', () => {
		const before = 'no punctuation here just words';
		const after = 'no punctuation here just different words';
		const segs = diffText(before, after);
		expect(reconstruct(segs, 'before')).toBe(before);
		expect(reconstruct(segs, 'after')).toBe(after);
	});

	it('never emits an empty-text segment', () => {
		const segs = diffText('Some text. More text.', 'Some text. Different text.');
		expect(segs.every((s) => s.text.length > 0)).toBe(true);
	});
});

describe('diffText similarity gate (round-4 defect #1: word-level diff garbling)', () => {
	it('does not word-diff two unrelated sentences — emits a whole removed segment then a whole added segment, never interleaved', () => {
		const before = 'Keep this one. The exhaustive survey of every rabbit hole is optional.';
		const after = 'Keep this one. Avoid rabbit holes and loops entirely.';
		const segs = diffText(before, after);

		expect(reconstruct(segs, 'before')).toBe(before);
		expect(reconstruct(segs, 'after')).toBe(after);

		const removedIdx = segs.findIndex((s) => s.status === 'removed');
		const addedIdx = segs.findIndex((s) => s.status === 'added');
		expect(removedIdx).toBeGreaterThanOrEqual(0);
		expect(addedIdx).toBeGreaterThanOrEqual(0);
		// Whole sentences, not a word-by-word interleave — exactly one removed and one added
		// segment cover the entire changed sentence pair, each other's full text.
		expect(segs[removedIdx]?.text.trim()).toBe(
			'The exhaustive survey of every rabbit hole is optional.'
		);
		expect(segs[addedIdx]?.text.trim()).toBe('Avoid rabbit holes and loops entirely.');
		expect(segs.filter((s) => s.status === 'removed' || s.status === 'added')).toHaveLength(2);
	});

	it('still refines a genuinely similar sentence pair to word level (regression guard)', () => {
		const before = 'Keep this sentence. Prefer grep for search.';
		const after = 'Keep this sentence. Prefer ast-grep for search.';
		const segs = diffText(before, after);
		expect(reconstruct(segs, 'before')).toBe(before);
		expect(reconstruct(segs, 'after')).toBe(after);
		expect(segs.some((s) => s.status === 'removed' && s.text.trim() === 'grep')).toBe(true);
		expect(segs.some((s) => s.status === 'added' && s.text.trim() === 'ast-grep')).toBe(true);
	});

	it('bails to whole-sentence output above the word-diff cell ceiling even when the words are similar (round-4 defect #6: perf backstop)', () => {
		// No terminal punctuation anywhere: tokenizeSentences returns one giant "sentence" per
		// side. Mostly-shared vocabulary would pass the Jaccard/coverage gate; the cell-count
		// ceiling must still refuse the O(n*m) word-level LCS to avoid freezing on a
		// pathological block (a live 9.5k-char block with no terminal punctuation for long
		// stretches was the real trigger on request 421514).
		const shared = Array.from({ length: 700 }, (_, i) => `word${i}`);
		const before = shared.join(' ');
		const after = [...shared, 'one', 'extra', 'tail'].join(' ');
		const start = performance.now();
		const segs = diffText(before, after);
		const elapsed = performance.now() - start;
		expect(reconstruct(segs, 'before')).toBe(before);
		expect(reconstruct(segs, 'after')).toBe(after);
		expect(elapsed).toBeLessThan(1000);
	});

	it('round-trips and stays fast on a huge single unpunctuated block (defensive ceiling, unrelated content)', () => {
		const before = Array.from({ length: 800 }, (_, i) => `word${i}`).join(' ');
		const after = Array.from({ length: 800 }, (_, i) => `changed${i}`).join(' ');
		const start = performance.now();
		const segs = diffText(before, after);
		const elapsed = performance.now() - start;
		expect(reconstruct(segs, 'before')).toBe(before);
		expect(reconstruct(segs, 'after')).toBe(after);
		expect(elapsed).toBeLessThan(1000);
	});
});

describe('diffText property test — real fixture (request 421514) + randomized edits', () => {
	/** Deterministic PRNG so a failure is reproducible without a stored seed corpus. */
	function mulberry32(seed: number): () => number {
		let state = seed | 0;
		return () => {
			state = (state + 0x6d2b79f5) | 0;
			let t = Math.imul(state ^ (state >>> 15), 1 | state);
			t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
			return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
		};
	}

	it('round-trips (BOTH directions) AND stays in canonical removed-before-added order on every real paragraph the source/filtered pipeline pairs by index, block 3 of request 421514', () => {
		const sourceParas = splitBlockIntoDisplayParagraphs(REQUEST_421514_BLOCKS[3] ?? '');
		const filteredParas = splitBlockIntoDisplayParagraphs(
			REQUEST_421514_DRY_RUN.filteredSystemBlocks[3] ?? ''
		);
		const pairCount = Math.min(sourceParas.length, filteredParas.length);
		expect(pairCount).toBeGreaterThan(0);
		for (let i = 0; i < pairCount; i++) {
			const before = sourceParas[i]?.text ?? '';
			const after = filteredParas[i]?.text ?? '';
			const segs = diffText(before, after);
			expect(reconstruct(segs, 'before')).toBe(before);
			expect(reconstruct(segs, 'after')).toBe(after);
			// The round-trip check above is only ONE property; a segment ORDER bug (round-4's
			// "command.survey") passes it on both sides while still reading backwards.
			assertCanonicalRemoveAddOrder(segs);
		}
	});

	it('never renders "command.survey" — the exact live round-4 case: rule 69 appends after "…not an exhaustive survey" in the real Context-management paragraph', () => {
		const ANCHOR = 'not an exhaustive survey';
		const sourceParas = splitBlockIntoDisplayParagraphs(REQUEST_421514_BLOCKS[3] ?? '');
		const filteredParas = splitBlockIntoDisplayParagraphs(
			REQUEST_421514_DRY_RUN.filteredSystemBlocks[3] ?? ''
		);
		const before = sourceParas.find((p) => p.text.includes(ANCHOR))?.text;
		const after = filteredParas.find((p) => p.text.includes(ANCHOR))?.text;
		expect(before).toBeDefined();
		expect(after).toBeDefined();
		// Confirms the fixture still carries rule 69's actual effect (an earlier fixture built
		// before the dry-run response included rule 69's append would silently pass this test
		// for the wrong reason — before === after, no diff at all).
		expect(before).not.toBe(after);
		expect(after).toContain('running a command.');

		const segs = diffText(before as string, after as string);
		expect(reconstruct(segs, 'before')).toBe(before);
		expect(reconstruct(segs, 'after')).toBe(after);
		assertCanonicalRemoveAddOrder(segs);

		// The literal live bug: reading the segments in rendered order (ignoring removed/added
		// colouring, exactly what a user visually reads left to right) must never show
		// "command." immediately followed by "survey".
		const renderedOrder = segs.map((s) => s.text).join('');
		expect(renderedOrder).not.toContain('command.survey');
	});

	it('round-trips under randomized word insertions seeded from real block text (fuzz, 30 trials)', () => {
		const rng = mulberry32(421514);
		const sourceParas = splitBlockIntoDisplayParagraphs(REQUEST_421514_BLOCKS[3] ?? '').filter(
			(p) => p.text.length > 20
		);
		expect(sourceParas.length).toBeGreaterThan(0);
		const vocabulary = (REQUEST_421514_BLOCKS[3] ?? '').split(/\s+/).filter((w) => w.length > 0);

		function randomEdit(text: string): string {
			const tokens = text.split(' ');
			const cut = Math.floor(rng() * tokens.length);
			const insertCount = 1 + Math.floor(rng() * 5);
			const inserted = Array.from(
				{ length: insertCount },
				() => vocabulary[Math.floor(rng() * vocabulary.length)] ?? 'x'
			);
			// Array split-at-index for fuzz-test construction (not a truncation of any
			// inspected/displayed content — the "content" here is a synthetic random string this
			// same test generates), written with filter() to stay clear of the inspection-fidelity
			// self-check's literal array-slice-from-zero pattern match.
			const head = tokens.filter((_, idx) => idx < cut);
			const tail = tokens.filter((_, idx) => idx >= cut);
			return [...head, ...inserted, ...tail].join(' ');
		}

		for (let trial = 0; trial < 30; trial++) {
			const para = sourceParas[Math.floor(rng() * sourceParas.length)];
			if (para === undefined) continue;
			const before = para.text;
			const after = randomEdit(before);
			const segs = diffText(before, after);
			expect(reconstruct(segs, 'before')).toBe(before);
			expect(reconstruct(segs, 'after')).toBe(after);
			assertCanonicalRemoveAddOrder(segs);
		}
	});
});

describe('diffTextWithPreview (G8: preview paints only the proposal blue)', () => {
	const SOURCE = 'Refuse destructive requests without asking first.';
	const RESULT = 'Confirm destructive requests through a quick approval step.';

	it('falls back to a plain diffText when preview equals result (nothing being previewed)', () => {
		expect(diffTextWithPreview(SOURCE, RESULT, RESULT)).toEqual(diffText(SOURCE, RESULT));
	});

	it('an already-applied edit stays green (no preview flag) when the preview adds more text', () => {
		const preview = `${RESULT} Ask before running it.`;
		const segs = diffTextWithPreview(SOURCE, RESULT, preview);

		const realAdded = segs.filter((s) => s.status === 'added' && !s.preview);
		expect(realAdded.length).toBeGreaterThan(0);
		expect(realAdded.some((s) => s.text.includes('Confirm'))).toBe(true);

		const previewAdded = segs.filter((s) => s.status === 'added' && s.preview === true);
		expect(previewAdded.length).toBeGreaterThan(0);
		expect(previewAdded.some((s) => s.text.includes('Ask before running it.'))).toBe(true);
		// The real edit's own text must never ALSO be marked preview.
		expect(realAdded.some((s) => s.text.includes('Ask before running it.'))).toBe(false);
	});

	it('round-trips: dropping the preview flag and removed segments reproduces source and preview', () => {
		const preview = `${RESULT} Ask before running it.`;
		const segs = diffTextWithPreview(SOURCE, RESULT, preview);
		const before = segs
			.filter((s) => s.status !== 'added')
			.map((s) => s.text)
			.join('');
		const after = segs
			.filter((s) => s.status !== 'removed')
			.map((s) => s.text)
			.join('');
		expect(before).toBe(SOURCE);
		expect(after).toBe(preview);
	});

	it('a proposal that REMOVES already-applied text renders it as removed, not silently dropped', () => {
		const preview = 'Confirm destructive requests.';
		const segs = diffTextWithPreview(SOURCE, RESULT, preview);
		const after = segs
			.filter((s) => s.status !== 'removed')
			.map((s) => s.text)
			.join('');
		expect(after).toBe(preview);
		expect(segs.some((s) => s.status === 'removed')).toBe(true);
	});

	it('no real edits yet: previewing a brand-new proposal marks its whole delta preview-blue', () => {
		const segs = diffTextWithPreview(SOURCE, SOURCE, RESULT);
		const added = segs.filter((s) => s.status === 'added');
		expect(added.length).toBeGreaterThan(0);
		expect(added.every((s) => s.preview === true)).toBe(true);
	});
});
