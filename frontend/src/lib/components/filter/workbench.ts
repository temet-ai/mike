import type { FilterRuleDto } from '$lib/types';
import { diffBlocks, type DiffRow } from '$lib/utils/blockDiff';
import { matches } from './rule-matching';

export interface SliceSelection {
	blockIndex: number;
	text: string;
	multiBlock: boolean;
	x: number;
	top: number;
	bottom: number;
}

export type PreviewStatus = 'idle' | 'loading' | 'ready' | 'unavailable' | 'error';

export type ScrollSyncSource = 'source' | 'preview';

export interface LineDivergence {
	status: DiffRow['status'];
	text: string;
}

export interface BlockDivergence {
	status: DiffRow['status'];
	changedLineCount: number;
	sourceLines: LineDivergence[];
	resultLines: LineDivergence[];
}

export interface PromptDivergence {
	source: Array<BlockDivergence | null>;
	result: Array<BlockDivergence | null>;
}

export function parseFromParam(raw: string | null): number | null {
	if (raw === null || raw.length === 0) return null;
	const match = raw.match(/^request:(\d+)$/);
	const idStr = match?.[1];
	if (idStr === undefined) return null;
	const id = Number.parseInt(idStr, 10);
	if (!Number.isFinite(id) || id < 0) return null;
	return id;
}

function splitLines(text: string): string[] {
	if (text.length === 0) return [];
	return text.split('\n');
}

function buildLineDivergence(rows: DiffRow[]): {
	sourceLines: LineDivergence[];
	resultLines: LineDivergence[];
	changedLineCount: number;
} {
	const sourceLines: LineDivergence[] = [];
	const resultLines: LineDivergence[] = [];
	let count = 0;

	for (const row of rows) {
		if (row.status === 'unchanged') {
			sourceLines.push({ status: 'unchanged', text: row.text });
			resultLines.push({ status: 'unchanged', text: row.text });
			continue;
		}

		if (row.status === 'replaced') {
			sourceLines.push({ status: 'replaced', text: row.originalText });
			resultLines.push({ status: 'replaced', text: row.sentText });
			count++;
			continue;
		}

		if (row.status === 'dropped') {
			sourceLines.push({ status: 'dropped', text: row.text });
			count++;
			continue;
		}

		resultLines.push({ status: 'appended', text: row.text });
		count++;
	}

	return { sourceLines, resultLines, changedLineCount: count };
}

export function computePromptDivergence(
	sourceBlocks: readonly string[],
	resultBlocks: readonly string[]
): PromptDivergence {
	const blockRows = diffBlocks({ original: [...sourceBlocks], sent: [...resultBlocks] });
	const source: Array<BlockDivergence | null> = Array.from(
		{ length: sourceBlocks.length },
		() => null
	);
	const result: Array<BlockDivergence | null> = Array.from(
		{ length: resultBlocks.length },
		() => null
	);

	for (const row of blockRows) {
		if (row.status === 'unchanged') {
			const lines = splitLines(row.text).map((text) => ({ status: 'unchanged' as const, text }));
			source[row.originalIndex] = {
				status: 'unchanged',
				changedLineCount: 0,
				sourceLines: lines,
				resultLines: lines
			};
			result[row.sentIndex] = {
				status: 'unchanged',
				changedLineCount: 0,
				sourceLines: lines,
				resultLines: lines
			};
			continue;
		}

		if (row.status === 'replaced') {
			const lineRows = diffBlocks({
				original: splitLines(row.originalText),
				sent: splitLines(row.sentText)
			});
			const lineDiff = buildLineDivergence(lineRows);
			source[row.originalIndex] = {
				status: 'replaced',
				changedLineCount: lineDiff.changedLineCount,
				sourceLines: lineDiff.sourceLines,
				resultLines: lineDiff.resultLines
			};
			result[row.sentIndex] = {
				status: 'replaced',
				changedLineCount: lineDiff.changedLineCount,
				sourceLines: lineDiff.sourceLines,
				resultLines: lineDiff.resultLines
			};
			continue;
		}

		if (row.status === 'dropped') {
			const sourceLines = splitLines(row.text).map((text) => ({
				status: 'dropped' as const,
				text
			}));
			source[row.originalIndex] = {
				status: 'dropped',
				changedLineCount: sourceLines.length,
				sourceLines,
				resultLines: []
			};
			continue;
		}

		const resultLines = splitLines(row.text).map((text) => ({ status: 'appended' as const, text }));
		result[row.sentIndex] = {
			status: 'appended',
			changedLineCount: resultLines.length,
			sourceLines: [],
			resultLines
		};
	}

	return { source, result };
}

export interface MatchSpan {
	start: number;
	end: number;
	rule: FilterRuleDto;
}

export function findOccurrences(
	haystack: string,
	needle: string
): { start: number; end: number }[] {
	if (needle.length === 0) return [];
	const hay = haystack.toLowerCase();
	const ndl = needle.toLowerCase();
	const out: { start: number; end: number }[] = [];
	let from = 0;
	for (;;) {
		const idx = hay.indexOf(ndl, from);
		if (idx === -1) break;
		out.push({ start: idx, end: idx + ndl.length });
		from = idx + ndl.length;
	}
	return out;
}

export interface BlockSegment {
	text: string;
	rule: FilterRuleDto | null;
}

export interface BlockMatchInfo {
	fullMatch: FilterRuleDto | null;
	segments: BlockSegment[];
}

export function computeBlockMatches(
	blockText: string,
	orderedRules: readonly FilterRuleDto[]
): BlockMatchInfo {
	const enabled = orderedRules.filter((r) => r.enabled);

	for (const rule of enabled) {
		if (rule.kind === 'xml_tag') {
			if (matches('xml_tag', rule.value, blockText)) {
				return { fullMatch: rule, segments: [{ text: blockText, rule: null }] };
			}
		} else if (
			rule.kind === 'text_substring' &&
			rule.value.trim().length > 0 &&
			rule.value.trim() === blockText.trim()
		) {
			return { fullMatch: rule, segments: [{ text: blockText, rule: null }] };
		}
	}

	const accepted: MatchSpan[] = [];
	for (const rule of enabled) {
		if (rule.kind !== 'text_substring') continue;
		for (const occ of findOccurrences(blockText, rule.value)) {
			const overlaps = accepted.some((a) => occ.start < a.end && a.start < occ.end);
			if (!overlaps) accepted.push({ ...occ, rule });
		}
	}
	accepted.sort((a, b) => a.start - b.start);

	const segments: BlockSegment[] = [];
	let cursor = 0;
	for (const span of accepted) {
		if (span.start > cursor) {
			segments.push({ text: blockText.slice(cursor, span.start), rule: null });
		}
		segments.push({ text: blockText.slice(span.start, span.end), rule: span.rule });
		cursor = span.end;
	}
	if (cursor < blockText.length) {
		segments.push({ text: blockText.slice(cursor), rule: null });
	}
	if (segments.length === 0) {
		segments.push({ text: blockText, rule: null });
	}
	return { fullMatch: null, segments };
}

export function countRuleMatches(
	rule: FilterRuleDto,
	blockTexts: readonly string[]
): { count: number; unit: 'block' | 'slice' } {
	if (rule.kind === 'xml_tag' || rule.kind === 'lazy_section') {
		let n = 0;
		for (const text of blockTexts) {
			if (matches(rule.kind, rule.value, text)) n++;
		}
		return { count: n, unit: 'block' };
	}
	let n = 0;
	for (const text of blockTexts) {
		n += findOccurrences(text, rule.value).length;
	}
	return { count: n, unit: 'slice' };
}

export function reconcileOrder(
	localOrder: readonly string[],
	rules: readonly FilterRuleDto[]
): string[] {
	const known = new Set(rules.map((r) => r.id));
	const kept = localOrder.filter((id) => known.has(id));
	const keptSet = new Set(kept);
	for (const rule of rules) {
		if (!keptSet.has(rule.id)) kept.push(rule.id);
	}
	return kept;
}

export function moveInOrder(order: readonly string[], id: string, delta: -1 | 1): string[] {
	const idx = order.indexOf(id);
	if (idx === -1) return [...order];
	const target = idx + delta;
	if (target < 0 || target >= order.length) return [...order];
	const next = [...order];
	const tmp = next[target] as string;
	next[target] = next[idx] as string;
	next[idx] = tmp;
	return next;
}

export function reorderByIndex(
	order: readonly string[],
	fromIndex: number,
	toIndex: number
): string[] {
	if (
		fromIndex < 0 ||
		fromIndex >= order.length ||
		toIndex < 0 ||
		toIndex >= order.length ||
		fromIndex === toIndex
	) {
		return [...order];
	}
	const next = [...order];
	const [moved] = next.splice(fromIndex, 1);
	if (moved === undefined) return [...order];
	next.splice(toIndex, 0, moved);
	return next;
}
