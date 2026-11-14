import type { FleetRecord, FleetSeatDecisionRow } from './types';

export interface SeatEffort {
	work: number;
	machinery: number;
	waiting: number;
}

export interface SeatDecisionEntry {
	id: number;
	at: string;
	result: string;
	score: number | null;
}

export interface SeatTriageEntry {
	id: number;
	at: string | null;
	exitCode: number | null;
	line: string;
}

export interface SeatDecisionFigures {
	effort: SeatEffort;
	mail: { held: number; woken: number };
	stopChecks: SeatDecisionEntry[];
	triage: SeatTriageEntry[];
	commitMessages: SeatDecisionEntry[];
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

const STOP_WORDS: Record<string, string> = {
	let_through: 'let through',
	pushed_back: 'pushed back'
};

const COMMIT_WORDS: Record<string, string> = {
	warned: 'warned'
};

function resultWord(
	row: FleetSeatDecisionRow,
	words: Record<string, string>,
	otherwise: string
): string {
	if (row.fail_open === true) return 'fell back';
	return (row.applied !== null ? words[row.applied] : undefined) ?? otherwise;
}

function entryOf(
	row: FleetSeatDecisionRow,
	words: Record<string, string>,
	otherwise: string
): SeatDecisionEntry {
	const score = row.verdict?.verdict?.score;
	return {
		id: row.id,
		at: row.created_at,
		result: resultWord(row, words, otherwise),
		score: typeof score === 'number' ? score : null
	};
}

function newestFirst<T extends { id: number; at: string | null }>(a: T, b: T): number {
	const left = a.at ?? '';
	const right = b.at ?? '';
	if (left !== right) return left < right ? 1 : -1;
	return b.id - a.id;
}

export function seatDecisionFigures(
	record: FleetRecord | null,
	seat: string | null
): SeatDecisionFigures | null {
	if (record === null || seat === null) return null;
	const labels = record.effort_split?.by_seat?.[seat]?.labels ?? {};
	const mail = record.mail_wake_counts?.by_seat?.[seat] ?? {};
	const points = record.seat_decisions?.by_seat?.[seat] ?? {};
	const triage = (record.seat_decisions?.check_triage ?? []).filter((t) => t.askers.includes(seat));
	return {
		effort: {
			work: labels['work']?.tokens ?? 0,
			machinery: labels['machinery']?.tokens ?? 0,
			waiting: labels['waiting']?.tokens ?? 0
		},
		mail: { held: mail['held'] ?? 0, woken: mail['sent'] ?? 0 },
		stopChecks: (points['fleet_stop_check'] ?? [])
			.map((r) => entryOf(r, STOP_WORDS, 'recorded only'))
			.sort(newestFirst),
		triage: triage
			.map((t) => ({ id: t.id, at: t.finished_at, exitCode: t.exit_code, line: t.triage }))
			.sort(newestFirst),
		commitMessages: (points['fleet_commit_message'] ?? [])
			.map((r) => entryOf(r, COMMIT_WORDS, 'no warning'))
			.sort(newestFirst)
	};
}

export function hasSeatRecord(record: FleetRecord | null, seat: string | null): boolean {
	if (record === null || seat === null) return false;
	return (
		record.effort_split?.by_seat?.[seat] !== undefined ||
		record.seat_decisions?.by_seat?.[seat] !== undefined ||
		(record.seat_decisions?.check_triage ?? []).some((t) => t.askers.includes(seat))
	);
}
