import { TURN_GUARD_POINT, type DecisionPointRow } from '$lib/api/decisions';
import { formatInterval, wilson } from './wilson';

export interface RightCell {
	counts: string;
	interval: string;
	holdSide: boolean;
}

export const NO_VALUE = '—';

export function rowLabel(row: DecisionPointRow): string {
	return row.label ?? row.point;
}

export function matchesFilter(row: DecisionPointRow, query: string): boolean {
	const q = query.trim().toLowerCase();
	if (q === '') return true;
	return `${rowLabel(row)} ${row.point}`.toLowerCase().includes(q);
}

export function rightCell(row: DecisionPointRow): RightCell {
	const score = row.score;
	if (score === null) return { counts: NO_VALUE, interval: '', holdSide: false };
	if (row.point === TURN_GUARD_POINT) {
		if (score.held_labelled === 0) return { counts: NO_VALUE, interval: '', holdSide: true };
		return {
			counts: `${score.held_wrong}/${score.held_labelled}`,
			interval: formatInterval(wilson(score.held_wrong, score.held_labelled)),
			holdSide: true
		};
	}
	if (score.applied_labelled === 0) return { counts: NO_VALUE, interval: '', holdSide: false };
	return {
		counts: `${score.applied_right}/${score.applied_labelled}`,
		interval: formatInterval(wilson(score.applied_right, score.applied_labelled)),
		holdSide: false
	};
}

export function failOpenPercent(row: DecisionPointRow): string {
	const counts = row.score?.counts;
	if (!counts || counts.calls <= 0) return '0%';
	return `${Math.round((100 * counts.fail_open) / counts.calls)}%`;
}

export function formatSpend(usd: number): string {
	return `$${usd.toFixed(4)}`;
}

export function formatMs(value: number | null): string {
	return value === null ? NO_VALUE : `${Math.round(value)} ms`;
}

export function groupTotals(rows: readonly DecisionPointRow[]): { calls: number; spend: number } {
	let calls = 0;
	let spend = 0;
	for (const row of rows) {
		calls += row.score?.counts.calls ?? 0;
		spend += row.score?.counts.cost_usd ?? 0;
	}
	return { calls, spend };
}

export function hasHistory(rows: readonly DecisionPointRow[]): boolean {
	return rows.some((row) => (row.score?.counts.calls ?? 0) > 0);
}
