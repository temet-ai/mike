/**
 * Word/sentence-granularity text diff for the "With my edits" and "Final" prompt lenses
 * (spec section 4.2). Two-tier LCS diff, mirroring the block-level algorithm in
 * `$lib/utils/blockDiff.ts` but applied within one paragraph:
 *
 *  1. Split the paragraph into sentences and diff at sentence granularity.
 *  2. A sentence that changed on both sides (a dropped sentence immediately followed by
 *     an appended one) is a candidate "replace" — but only refined to word granularity when
 *     the two sentences are actually similar enough to be an edit of each other
 *     (`tryWordDiff`'s Jaccard/coverage gate, added 2026-09-23). Word-level LCS can always
 *     find SOME accidental shared word ("the", "a", a generic term) between two totally
 *     unrelated sentences, and anchoring on it interleaves their unrelated content into a
 *     confusing word salad — this was a real bug seen live on request 421514, where two
 *     unrelated paragraphs' text interleaved word-by-word. Below the similarity threshold the
 *     pair is emitted as a whole removed sentence followed by a whole added sentence instead.
 *  3. A sentence added or removed outright (no paired counterpart) stays sentence-granular:
 *     the whole sentence is one added/removed segment.
 *
 * Every function is pure and whitespace-preserving: joining a segment list's `text`
 * fields back together reproduces the input exactly (no `…`, no lost characters).
 */

export type DiffSegmentStatus = 'unchanged' | 'removed' | 'added';

export interface DiffSegment {
	text: string;
	status: DiffSegmentStatus;
	/**
	 * Set on an `added` segment that exists ONLY because of a proposal being previewed — never
	 * on a segment that was already part of the current edits before the preview started. Lets
	 * the prompt pane paint just the proposal's own delta blue while an already-applied edit
	 * stays its normal green (G8: the old code recoloured EVERY added segment blue the moment
	 * any preview was active, live on request 421514).
	 */
	preview?: boolean;
}

/**
 * Each token is a word PLUS its trailing whitespace, never bare whitespace on its own
 * (except a leading run before the first word, if any). Splitting whitespace into its own
 * free-floating token would let the LCS pass "match" the space between two completely
 * unrelated words purely because both sides have a space there, fragmenting a clean
 * whole-phrase replacement into a confusing word-by-word interleave. Bundling the
 * whitespace onto its word removes that false anchor.
 */
function tokenizeWords(text: string): string[] {
	return text.match(/\S+\s*|\s+/g) ?? [];
}

const TERMINAL_PUNCTUATION = /[.!?]+/g;

/**
 * Splits on sentence-ending punctuation, WITHOUT swallowing the whitespace that follows
 * it — that whitespace prefixes the next sentence instead. This keeps a sentence's own
 * token identical whether or not another sentence follows it, so the LCS pass above
 * doesn't see two unrelated tokens where the wording is actually unchanged (a real bug
 * caught by promptDiff.test.ts: trailing-whitespace differences broke sentence equality
 * and mis-attributed a whole added sentence as a partial word-level change).
 */
function tokenizeSentences(text: string): string[] {
	if (text.length === 0) return [];
	const out: string[] = [];
	let start = 0;
	TERMINAL_PUNCTUATION.lastIndex = 0;
	let match: RegExpExecArray | null;
	while ((match = TERMINAL_PUNCTUATION.exec(text)) !== null) {
		const end = match.index + match[0].length;
		out.push(text.slice(start, end));
		start = end;
	}
	if (start < text.length) out.push(text.slice(start));
	return out;
}

type LcsOp<T> =
	| { status: 'unchanged'; value: T }
	| { status: 'removed'; value: T }
	| { status: 'added'; value: T };

function lcsDiff<T>(a: readonly T[], b: readonly T[], eq: (x: T, y: T) => boolean): LcsOp<T>[] {
	const n = a.length;
	const m = b.length;
	if (n === 0 && m === 0) return [];
	if (n === 0) return b.map((value) => ({ status: 'added' as const, value }));
	if (m === 0) return a.map((value) => ({ status: 'removed' as const, value }));

	const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
	for (let i = 1; i <= n; i++) {
		for (let j = 1; j <= m; j++) {
			const prevRow = dp[i - 1];
			const curRow = dp[i];
			if (prevRow === undefined || curRow === undefined) continue;
			if (eq(a[i - 1] as T, b[j - 1] as T)) {
				curRow[j] = (prevRow[j - 1] ?? 0) + 1;
			} else {
				curRow[j] = Math.max(prevRow[j] ?? 0, curRow[j - 1] ?? 0);
			}
		}
	}

	const reversed: LcsOp<T>[] = [];
	let i = n;
	let j = m;
	while (i > 0 && j > 0) {
		const rowI = dp[i];
		const rowIm1 = dp[i - 1];
		if (rowI === undefined || rowIm1 === undefined) break;
		if (eq(a[i - 1] as T, b[j - 1] as T)) {
			reversed.push({ status: 'unchanged', value: a[i - 1] as T });
			i--;
			j--;
		} else if ((rowIm1[j] ?? 0) >= (rowI[j - 1] ?? 0)) {
			reversed.push({ status: 'removed', value: a[i - 1] as T });
			i--;
		} else {
			reversed.push({ status: 'added', value: b[j - 1] as T });
			j--;
		}
	}
	while (i > 0) {
		reversed.push({ status: 'removed', value: a[i - 1] as T });
		i--;
	}
	while (j > 0) {
		reversed.push({ status: 'added', value: b[j - 1] as T });
		j--;
	}
	return reversed.reverse();
}

function mergeRuns(ops: readonly LcsOp<string>[]): DiffSegment[] {
	const out: DiffSegment[] = [];
	for (const op of ops) {
		const last = out[out.length - 1];
		if (last !== undefined && last.status === op.status) {
			last.text += op.value;
		} else {
			out.push({ text: op.value, status: op.status });
		}
	}
	return out;
}

/** Word-granularity diff (whitespace tokens preserved), no sentence pass. */
export function diffWords(before: string, after: string): DiffSegment[] {
	if (before === after) return before.length > 0 ? [{ text: before, status: 'unchanged' }] : [];
	return finalizeSegments(
		mergeRuns(lcsDiff(tokenizeWords(before), tokenizeWords(after), (a, b) => a === b))
	);
}

/** A removed+added sentence pair only gets word-level refinement above this token overlap. */
const SIMILARITY_JACCARD_THRESHOLD = 0.5;
/** ...or when the shorter side's words are at least this fraction covered by the LCS. */
const SIMILARITY_COVERAGE_THRESHOLD = 0.5;
/**
 * Word-level LCS is O(tokensBefore * tokensAfter) cells. A "sentence" from `tokenizeSentences`
 * can be arbitrarily large when a block has long stretches with no terminal punctuation (a
 * bullet list, a code block) — one 9.5k-char block in a live capture (request 421514) produced
 * a single several-thousand-word "sentence". Past this many cells, skip word-level refinement
 * outright regardless of similarity: a defensive ceiling against freezing the tab on a
 * pathological block, independent of the similarity gate above.
 */
const WORD_DIFF_CELL_CEILING = 250_000;

function wordSet(tokens: readonly string[]): Set<string> {
	const out = new Set<string>();
	for (const t of tokens) {
		const trimmed = t.trim().toLowerCase();
		if (trimmed.length > 0) out.add(trimmed);
	}
	return out;
}

function jaccard(a: ReadonlySet<string>, b: ReadonlySet<string>): number {
	if (a.size === 0 && b.size === 0) return 1;
	let intersection = 0;
	for (const t of a) if (b.has(t)) intersection++;
	const union = a.size + b.size - intersection;
	return union === 0 ? 1 : intersection / union;
}

/**
 * Word-diffs a removed+added sentence pair, but only when they're similar enough to be an
 * edit of each other. Returns `null` when the pair should instead stay whole-sentence
 * (unrelated sentences, or a pair too large to safely LCS at word granularity).
 */
function tryWordDiff(before: string, after: string): DiffSegment[] | null {
	const beforeTokens = tokenizeWords(before);
	const afterTokens = tokenizeWords(after);
	if (beforeTokens.length === 0 || afterTokens.length === 0) return null;
	if (beforeTokens.length * afterTokens.length > WORD_DIFF_CELL_CEILING) return null;

	const ops = lcsDiff(beforeTokens, afterTokens, (a, b) => a === b);
	const commonCount = ops.reduce((n, op) => (op.status === 'unchanged' ? n + 1 : n), 0);
	const shorter = Math.min(beforeTokens.length, afterTokens.length);
	const coverage = shorter === 0 ? 0 : commonCount / shorter;
	const similarity = jaccard(wordSet(beforeTokens), wordSet(afterTokens));

	if (similarity < SIMILARITY_JACCARD_THRESHOLD && coverage < SIMILARITY_COVERAGE_THRESHOLD) {
		return null;
	}
	return finalizeSegments(mergeRuns(ops));
}

/**
 * Forces a stable reading order on any contiguous run of removed/added segments (no
 * `unchanged` between them): every removed piece before every added piece, each preserving
 * its own relative order. `lcsDiff`'s backtrack has no preference between "removed" and
 * "added" when two runs share no further common tokens (both directions cost the same), so it
 * can legitimately emit added-then-removed for a tail like before="...survey" / after="...survey
 * Think before acting … running a command." — round-trip-correct (concatenating either side
 * reproduces its source exactly) but, rendered in that order, reads as "…running a
 * command." immediately followed by a struck-through leftover "survey" (a real bug seen live on
 * request 421514, rule 69's append). Reordering to removed-then-added is the only reading order
 * that matches how a human describes an edit ("was X, now Y") and never changes what either
 * side's reconstruction contains.
 */
function canonicalizeRemoveAddOrder(segments: readonly DiffSegment[]): DiffSegment[] {
	const out: DiffSegment[] = [];
	let i = 0;
	while (i < segments.length) {
		const seg = segments[i];
		if (seg === undefined) break;
		if (seg.status === 'unchanged') {
			out.push(seg);
			i++;
			continue;
		}
		const run: DiffSegment[] = [];
		while (i < segments.length) {
			const s = segments[i];
			if (s === undefined || s.status === 'unchanged') break;
			run.push(s);
			i++;
		}
		const removed = run.filter((s) => s.status === 'removed');
		const added = run.filter((s) => s.status === 'added');
		out.push(...removed, ...added);
	}
	return out;
}

/** Canonical order, then re-merge — reordering can put two same-status segments adjacent
 *  again (e.g. two removed pieces from different sub-runs), which must collapse into one. */
function finalizeSegments(segments: readonly DiffSegment[]): DiffSegment[] {
	return mergeAdjacentSameStatus(canonicalizeRemoveAddOrder(segments));
}

/**
 * Two-tier sentence/word diff — the function the prompt pane uses for a changed block.
 * A sentence-level "replace" (adjacent removed+added sentence) is refined into a word diff
 * only when `tryWordDiff` finds the pair similar enough to be an edit of each other; otherwise
 * the old sentence is emitted whole-removed and the new sentence whole-added, un-interleaved.
 */
export function diffText(before: string, after: string): DiffSegment[] {
	if (before === after) return before.length > 0 ? [{ text: before, status: 'unchanged' }] : [];

	const sentenceOps = lcsDiff(
		tokenizeSentences(before),
		tokenizeSentences(after),
		(a, b) => a === b
	);

	const merged: DiffSegment[] = [];
	let k = 0;
	while (k < sentenceOps.length) {
		const cur = sentenceOps[k];
		const next = sentenceOps[k + 1];
		if (cur === undefined) break;
		if (cur.status === 'removed' && next !== undefined && next.status === 'added') {
			const wordDiff = tryWordDiff(cur.value, next.value);
			if (wordDiff !== null) {
				merged.push(...wordDiff);
			} else {
				merged.push({ text: cur.value, status: 'removed' });
				merged.push({ text: next.value, status: 'added' });
			}
			k += 2;
			continue;
		}
		if (cur.status === 'added' && next !== undefined && next.status === 'removed') {
			const wordDiff = tryWordDiff(next.value, cur.value);
			if (wordDiff !== null) {
				merged.push(...wordDiff);
			} else {
				merged.push({ text: next.value, status: 'removed' });
				merged.push({ text: cur.value, status: 'added' });
			}
			k += 2;
			continue;
		}
		merged.push({ text: cur.value, status: cur.status });
		k++;
	}

	return finalizeSegments(merged);
}

function mergeAdjacentSameStatus(segments: readonly DiffSegment[]): DiffSegment[] {
	const out: DiffSegment[] = [];
	for (const seg of segments) {
		if (seg.text.length === 0) continue;
		const last = out[out.length - 1];
		if (last !== undefined && last.status === seg.status) {
			last.text += seg.text;
		} else {
			out.push({ ...seg });
		}
	}
	return out;
}

function mergeAdjacentPreviewAware(segments: readonly DiffSegment[]): DiffSegment[] {
	const out: DiffSegment[] = [];
	for (const seg of segments) {
		if (seg.text.length === 0) continue;
		const last = out[out.length - 1];
		if (last !== undefined && last.status === seg.status && !!last.preview === !!seg.preview) {
			last.text += seg.text;
		} else {
			out.push({ ...seg });
		}
	}
	return out;
}

/**
 * The "per-block two-level diff" G8 asks for: paints ONLY the segments a proposal being
 * PREVIEWED actually changes in blue, leaving edits that were already applied before the
 * preview started in their normal green — the bug was `WorkbenchSourcePane` recolouring EVERY
 * `added` segment blue the instant any preview was active, live on request 421514.
 *
 * Two ordinary diffs, reconciled onto one coordinate space:
 *  1. `diffText(source, result)` — the REAL edits, always correct, never touched by previewing.
 *  2. `diffText(result, preview)` — what the proposal alone changes.
 *
 * (1)'s `unchanged`/`added` segments, concatenated in order, reconstruct `result` exactly
 * (`diffText`'s own round-trip guarantee), so they can be laid out as `[start, end)` ranges
 * over `result`. (2)'s `unchanged`/`removed` segments reconstruct `result` too, in the same
 * coordinate space. Walking (2) in order: an `added` segment (new text (2) introduces) is
 * preview-blue outright; a `removed` segment (text (2) drops) renders `removed`, since it
 * doesn't survive into what's actually being previewed; an `unchanged` segment is sliced by
 * (1)'s ranges so its `unchanged`/`added`(real, green) status survives into the merged output.
 *
 * Falls back to a plain `diffText(source, result)` when `result === preview` (nothing to
 * preview yet, or the proposal doesn't change this text) so the non-preview render path is
 * untouched.
 */
export function diffTextWithPreview(
	source: string,
	result: string,
	preview: string
): DiffSegment[] {
	if (result === preview) return diffText(source, result);

	const diffA = diffText(source, result);
	const diffB = diffText(result, preview);

	interface OffsetSeg {
		text: string;
		status: DiffSegmentStatus;
		start: number;
		end: number;
	}
	// `aSegs` covers `result` — diffA's `unchanged`/`added` segments reconstruct it in order.
	// `removedBefore[i]` is the text diffA REMOVED FROM SOURCE immediately before `aSegs[i]`
	// (present in `source`, absent from `result` — a real edit's own drop, untouched by
	// whatever's being previewed, since it never made it into `result` to begin with).
	// `trailingRemoved` holds any such text after the LAST `aSegs` entry. Both must still surface
	// in the final output — the first version of this function silently dropped them, breaking
	// the "before"/"after" round-trip the same way `diffText` itself guarantees.
	const aSegs: OffsetSeg[] = [];
	const removedBefore: string[] = [];
	let trailingRemoved = '';
	{
		let pos = 0;
		let pending = '';
		for (const seg of diffA) {
			if (seg.status === 'removed') {
				pending += seg.text;
				continue;
			}
			aSegs.push({ text: seg.text, status: seg.status, start: pos, end: pos + seg.text.length });
			removedBefore.push(pending);
			pending = '';
			pos += seg.text.length;
		}
		trailingRemoved = pending;
	}

	const emittedRemovedBefore = new Set<number>();

	function sliceA(start: number, end: number): DiffSegment[] {
		const out: DiffSegment[] = [];
		for (let i = 0; i < aSegs.length; i++) {
			const seg = aSegs[i];
			if (seg === undefined) continue;
			const s = Math.max(seg.start, start);
			const e = Math.min(seg.end, end);
			if (s >= e) continue;
			if (s === seg.start && !emittedRemovedBefore.has(i)) {
				emittedRemovedBefore.add(i);
				const pre = removedBefore[i] ?? '';
				if (pre.length > 0) out.push({ text: pre, status: 'removed' });
			}
			out.push({ text: result.slice(s, e), status: seg.status });
		}
		return out;
	}

	const out: DiffSegment[] = [];
	let resultPos = 0;
	for (const seg of diffB) {
		if (seg.status === 'added') {
			out.push({ text: seg.text, status: 'added', preview: true });
			continue;
		}
		const start = resultPos;
		const end = resultPos + seg.text.length;
		resultPos = end;
		if (seg.status === 'removed') {
			out.push({ text: seg.text, status: 'removed' });
			continue;
		}
		out.push(...sliceA(start, end));
	}
	if (trailingRemoved.length > 0) out.push({ text: trailingRemoved, status: 'removed' });
	return mergeAdjacentPreviewAware(out);
}
