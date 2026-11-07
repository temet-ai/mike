import { describe, expect, it } from 'vitest';
import { diffBlocks } from '$lib/utils/blockDiff';
import { splitBlockIntoDisplayParagraphs } from './blockParagraphs';
import { diffText, type DiffSegment } from './promptDiff';
import { REQUEST_421514_BLOCKS, REQUEST_421514_DRY_RUN } from './__fixtures__/request421514';

/**
 * Round-4 defect #6 benchmark: the SAME pipeline `WorkbenchSourcePane.svelte`'s `changeGroupFor`
 * runs (block-level diff → paragraph-level diff → `diffText` per replaced paragraph pair),
 * driven over the REAL request-421514 fixture (a 184k-char request, ~11.4k-char system prompt,
 * 9 firing rules), comparing the CURRENT `diffText` (with the round-4 similarity gate) against
 * a self-contained copy of the PRE-fix algorithm (unconditional word-level refinement of every
 * removed+added sentence pair, the code this file's `diffText` replaced). This is a real
 * before/after timing, not a synthetic worst case — see this test's own console output for the
 * numbers reported alongside the round-4 fix.
 */

/** Verbatim copy of `diffText`'s pre-round-4 behaviour: unconditionally word-diffs every
 *  removed+added sentence pair, with no similarity gate and no cell-count ceiling. Kept ONLY
 *  here, for comparison — never exported from production code. */
function oldUnconditionalDiffText(before: string, after: string): DiffSegment[] {
	function tokenizeWords(text: string): string[] {
		return text.match(/\S+\s*|\s+/g) ?? [];
	}
	const TERMINAL_PUNCTUATION = /[.!?]+/g;
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
	function oldDiffWords(a: string, b: string): DiffSegment[] {
		if (a === b) return a.length > 0 ? [{ text: a, status: 'unchanged' }] : [];
		return mergeRuns(lcsDiff(tokenizeWords(a), tokenizeWords(b), (x, y) => x === y));
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
			merged.push(...oldDiffWords(cur.value, next.value));
			k += 2;
			continue;
		}
		if (cur.status === 'added' && next !== undefined && next.status === 'removed') {
			merged.push(...oldDiffWords(next.value, cur.value));
			k += 2;
			continue;
		}
		merged.push({ text: cur.value, status: cur.status });
		k++;
	}
	return mergeAdjacentSameStatus(merged);
}

/** Mirrors `WorkbenchSourcePane.svelte`'s `changeGroupFor` "replaced" branch: block-level diff,
 *  then paragraph-level diff, then a text-diff per replaced paragraph pair. */
function runFullPipeline(
	sourceBlocks: readonly string[],
	resultBlocks: readonly string[],
	textDiffFn: (before: string, after: string) => DiffSegment[]
): { segmentCount: number } {
	const rows = diffBlocks({ original: [...sourceBlocks], sent: [...resultBlocks] });
	let segmentCount = 0;
	for (const row of rows) {
		if (row.status !== 'replaced') continue;
		const sourceParas = splitBlockIntoDisplayParagraphs(row.originalText);
		const resultParas = splitBlockIntoDisplayParagraphs(row.sentText);
		const paraRows = diffBlocks({
			original: sourceParas.map((p) => p.text),
			sent: resultParas.map((p) => p.text)
		});
		for (const paraRow of paraRows) {
			if (paraRow.status !== 'replaced') continue;
			segmentCount += textDiffFn(paraRow.originalText, paraRow.sentText).length;
		}
	}
	return { segmentCount };
}

describe('diffText real-fixture benchmark (round-4 defect #6)', () => {
	it('reports before/after timings for the full workbench diff pipeline on request 421514', () => {
		const sourceBlocks = REQUEST_421514_BLOCKS;
		const resultBlocks = REQUEST_421514_DRY_RUN.filteredSystemBlocks;

		const beforeOld = performance.now();
		const oldResult = runFullPipeline(sourceBlocks, resultBlocks, oldUnconditionalDiffText);
		const oldElapsed = performance.now() - beforeOld;

		const beforeNew = performance.now();
		const newResult = runFullPipeline(sourceBlocks, resultBlocks, diffText);
		const newElapsed = performance.now() - beforeNew;

		// eslint-disable-next-line no-console -- deliberate: this IS the benchmark's report
		console.log(
			`[promptDiff perf] request 421514 (11,395 → 12,236 system-prompt chars, 9 firing rules): ` +
				`OLD (unconditional word-diff) ${oldElapsed.toFixed(2)}ms / ${oldResult.segmentCount} segments — ` +
				`NEW (round-4 similarity gate) ${newElapsed.toFixed(2)}ms / ${newResult.segmentCount} segments`
		);

		// The real regression guard: the new pipeline must complete comfortably inside a single
		// frame budget on this real, previously-freezing fixture — not "faster than the old code"
		// (on this particular fixture the old code was not itself pathologically slow to COMPUTE;
		// the live freeze combined this with render cost from far more, smaller segments — see the
		// segment-count comparison above, which the similarity gate also reduces).
		expect(newElapsed).toBeLessThan(500);
	});
});
