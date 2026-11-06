import type { EvalLabelDto, VocabularyAxis } from '$lib/api/eval';

export const LABEL_WORDS: Record<string, string> = {
	corrected: 'Corrected',
	redirected: 'Redirected',
	accepted: 'Accepted',
	no_response: 'No response',
	unclear: 'Unclear',
	shipped: 'Shipped',
	delivered: 'Delivered',
	partial: 'Partial',
	none: 'None',
	question: 'Question',
	not_applicable: 'Not applicable',
	done: 'Done',
	closed_in_frustration: 'Closed in frustration',
	cut_off_neutral: 'Cut off, neutral',
	fleet_seat: 'Fleet seat',
	success: 'Success',
	failure: 'Failure',
	unknown: 'Unknown'
};

export const AXIS_WORDS: Record<string, string> = {
	reaction: 'How you reacted',
	delivery: 'What was delivered',
	session_end: 'How it ended'
};

export const TIER_WORDS: Record<string, string> = {
	gold: 'Yours',
	relayed: 'Relayed',
	auto_shadow: 'Automatic',
	seed: 'Model'
};

export const SPLIT_WORDS: Record<string, string> = {
	holdout: 'Measurement only',
	dev: ''
};

function spelled(raw: string): string {
	const spaced = raw.replaceAll('_', ' ');
	return spaced.replace(/^./, (first) => first.toUpperCase());
}

export function wordFor(label: string): string {
	return LABEL_WORDS[label] ?? spelled(label);
}

export function axisWord(axis: string): string {
	return AXIS_WORDS[axis] ?? spelled(axis);
}

export function tierWord(tier: string): string {
	return TIER_WORDS[tier] ?? spelled(tier);
}

export function splitWord(split: string): string {
	return SPLIT_WORDS[split] ?? spelled(split);
}

export function optionsFor(vocabulary: VocabularyAxis[], axis: string) {
	const found = vocabulary.find((a) => a.axis === axis);
	return found ? [...found.labels].sort((a, b) => a.sort_order - b.sort_order) : [];
}

export function currentGold(labels: EvalLabelDto[], axis: string): EvalLabelDto | null {
	return (
		labels
			.filter((l) => l.axis === axis && l.tier === 'gold' && l.action === 'assert' && l.is_current)
			.sort((a, b) => b.id - a.id)[0] ?? null
	);
}

export function changedEntries(
	selection: Record<string, string | null>,
	labels: EvalLabelDto[]
): Array<{ axis: string; label: string }> {
	return Object.entries(selection)
		.filter((entry): entry is [string, string] => entry[1] !== null)
		.filter(([axis, label]) => currentGold(labels, axis)?.label !== label)
		.map(([axis, label]) => ({ axis, label }));
}

export type SavePlan =
	| { kind: 'labels'; entries: Array<{ axis: string; label: string }> }
	| { kind: 'note'; title: string; body: string }
	| { kind: 'nothing'; reason: 'choose' | 'note-needs-label' | 'unchanged' };

export function firstLine(text: string): string {
	return (
		text
			.split('\n')
			.map((line) => line.trim())
			.find((line) => line !== '') ?? ''
	);
}

export function planSave(
	selection: Record<string, string | null>,
	labels: EvalLabelDto[],
	note: string,
	hasSession: boolean
): SavePlan {
	const body = note.trim();
	const selected = Object.entries(selection).filter(
		(entry): entry is [string, string] => entry[1] !== null
	);
	const changed = changedEntries(selection, labels);
	if (changed.length > 0) return { kind: 'labels', entries: changed };
	if (selected.length > 0) {
		if (body === '') return { kind: 'nothing', reason: 'unchanged' };
		return { kind: 'labels', entries: selected.map(([axis, label]) => ({ axis, label })) };
	}
	if (body === '') return { kind: 'nothing', reason: 'choose' };
	if (!hasSession) return { kind: 'nothing', reason: 'note-needs-label' };
	return { kind: 'note', title: firstLine(body), body };
}

export function noteTurnId(note: { data: unknown }): number | null {
	const data = note.data;
	if (typeof data !== 'object' || data === null) return null;
	const id = (data as Record<string, unknown>)['turn_id'];
	return typeof id === 'number' ? id : null;
}

export function notesForSubject<T extends { data: unknown; created_at: string }>(
	notes: T[],
	turnId: number | null
): T[] {
	return notes
		.filter((note) => noteTurnId(note) === turnId)
		.sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at));
}

export function newestFirst(labels: EvalLabelDto[]): EvalLabelDto[] {
	return [...labels].sort((a, b) => b.id - a.id);
}

export function canWithdraw(label: EvalLabelDto): boolean {
	return (
		label.is_current === true &&
		label.action === 'assert' &&
		(label.tier === 'gold' || label.tier === 'relayed')
	);
}
