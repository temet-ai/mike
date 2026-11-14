import type { FleetRecord } from './types';

export interface SeatEffort {
	work: number;
	machinery: number;
	waiting: number;
}

export interface SeatDecisionFigures {
	effort: SeatEffort;
	mail: { held: number; woken: number };
}

export function wholePercents(values: readonly number[]): number[] {
	const total = values.reduce((sum, v) => sum + Math.max(0, v), 0);
	if (!(total > 0)) return values.map(() => 0);
	const exact = values.map((v) => (Math.max(0, v) / total) * 100);
	const floors = exact.map(Math.floor);
	let missing = 100 - floors.reduce((sum, v) => sum + v, 0);
	const byRemainder = exact
		.map((value, index) => ({ index, remainder: value - Math.floor(value) }))
		.sort((a, b) => b.remainder - a.remainder || a.index - b.index);
	for (const { index } of byRemainder) {
		if (missing <= 0) break;
		floors[index] = (floors[index] ?? 0) + 1;
		missing -= 1;
	}
	return floors;
}

export function seatDecisionFigures(
	record: FleetRecord | null,
	seat: string | null
): SeatDecisionFigures | null {
	if (record === null || seat === null) return null;
	const labels = record.effort_split?.by_seat?.[seat]?.labels ?? {};
	const mail = record.mail_wake_counts?.by_seat?.[seat] ?? {};
	return {
		effort: {
			work: labels['work']?.tokens ?? 0,
			machinery: labels['machinery']?.tokens ?? 0,
			waiting: labels['waiting']?.tokens ?? 0
		},
		mail: { held: mail['held'] ?? 0, woken: mail['sent'] ?? 0 }
	};
}
