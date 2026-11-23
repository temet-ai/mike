import {
	Check,
	CircleCheck,
	CircleDashed,
	CircleX,
	CopyCheck,
	FilePen,
	Merge,
	Minus,
	Replace,
	Undo2
} from '@lucide/svelte';
import type { StatusMarkTone } from '$lib/components/ui/status-mark';
import type { CapabilityRow } from '$lib/api/capabilities';
import type {
	Application,
	DecisionRow,
	MeasureSeriesRow,
	StatementState,
	SubjectStatement,
	Verdict
} from '$lib/api/knowledgeConsolidation';
import {
	formatSignedCount,
	formatSignedDecimal,
	formatSignedPoints,
	type TrendDirection
} from '$lib/components/ui/trend-chart';

type Glyph = typeof Check;

export interface VerdictLabel {
	word: string;
	tooltip: string;
}

export const VERDICTS: Record<Verdict, VerdictLabel> = {
	kept: { word: 'Kept', tooltip: 'Judged and left as it is.' },
	covered: {
		word: 'Covered',
		tooltip: 'Another statement says the same thing, so this one was retired.'
	},
	superseded: {
		word: 'Superseded',
		tooltip:
			'It contradicted another statement and lost by the precedence rules, so it was retired.'
	},
	merged: {
		word: 'Merged',
		tooltip: 'Its detail was folded into another lesson, and this lesson was retired.'
	},
	tightened: {
		word: 'Shortened',
		tooltip: 'Rewritten shorter. A second check found no instruction lost.'
	},
	misgrouped: {
		word: 'Moved',
		tooltip: 'The judge said it does not belong in this subject. It is grouped again.'
	},
	conflict_open: {
		word: 'Conflict',
		tooltip: 'It contradicts another statement and no rule settles it.'
	}
};

/** The verdicts the default "Changes" view lists. */
export const CHANGE_VERDICTS: Verdict[] = [
	'covered',
	'superseded',
	'merged',
	'tightened',
	'misgrouped'
];

export const VERDICT_ORDER: Verdict[] = [
	'kept',
	'covered',
	'superseded',
	'merged',
	'tightened',
	'misgrouped',
	'conflict_open'
];

export interface ApplicationLabel {
	icon: Glyph;
	tone: StatusMarkTone;
	word: string;
}

export const APPLICATIONS: Record<Application, ApplicationLabel> = {
	applied: { icon: Check, tone: 'strong', word: 'Applied' },
	proposed: { icon: FilePen, tone: 'info', word: 'Proposed' },
	failed: { icon: CircleX, tone: 'danger', word: 'Failed' },
	undone: { icon: Undo2, tone: 'quiet', word: 'Undone' },
	none: { icon: Minus, tone: 'quiet', word: 'No edit' }
};

export const PROPOSED_TOOLTIP =
	'mike may not write to this file by itself. Write it here, or leave it.';

export interface RowAction {
	kind: 'undo' | 'apply';
	label: string;
	variant: 'outline' | 'default';
	tooltip: string;
}

export function actionFor(row: DecisionRow): RowAction | null {
	const path = row.carrier?.relative_path ?? 'the file';
	switch (row.application) {
		case 'applied':
			return {
				kind: 'undo',
				label: 'Undo',
				variant: 'outline',
				tooltip: 'Puts back the text stored before this change.'
			};
		case 'proposed':
			return {
				kind: 'apply',
				label: 'Write to file',
				variant: 'default',
				tooltip: `Writes this edit into ${path}.`
			};
		case 'failed':
			return {
				kind: 'apply',
				label: 'Try again',
				variant: 'outline',
				tooltip: `Writes this edit into ${path} again.`
			};
		default:
			return null;
	}
}

export function refusalFor(kind: 'undo' | 'apply' | 'keep', message: string): string {
	if (kind === 'undo') return `Not undone. ${message}`;
	if (kind === 'keep') return `Not kept. ${message}`;
	return `Not written. ${message}`;
}

export interface StatementMark {
	icon: Glyph;
	tone: StatusMarkTone;
	word: string;
	tooltip?: string;
}

export const STATEMENT_STATES: Record<StatementState, StatementMark> = {
	current: { icon: CircleCheck, tone: 'strong', word: 'In force' },
	covered: { icon: CopyCheck, tone: 'quiet', word: 'Covered' },
	superseded: { icon: Replace, tone: 'quiet', word: 'Superseded' },
	merged: { icon: Merge, tone: 'quiet', word: 'Merged' },
	gone: {
		icon: CircleDashed,
		tone: 'quiet',
		word: 'Gone',
		tooltip: 'Its file was split again and this passage no longer exists.'
	}
};

export function isRetired(state: StatementState): boolean {
	return state !== 'current';
}

/** Statement ids that sit in an open conflict, from the open conflict decisions. */
export function conflictedStatementIds(conflicts: DecisionRow[]): Set<number> {
	const ids = new Set<number>();
	for (const c of conflicts) {
		if (c.statement_id !== null) ids.add(c.statement_id);
		if (c.counterpart_statement_id !== null) ids.add(c.counterpart_statement_id);
	}
	return ids;
}

export function carrierOfStatement(
	statements: SubjectStatement[],
	statementId: number | null
): SubjectStatement | null {
	if (statementId === null) return null;
	return statements.find((s) => s.statement_id === statementId) ?? null;
}

export function settlesLine(a: DecisionRow): string {
	const files = a.carrier?.kind === 'file' && a.counterpart?.kind === 'file';
	return files
		? 'Both files were written within a day of each other, and nothing you typed later picks one. Keep one here, or edit one of the files.'
		: 'Both lessons were stated within a day of each other, and nothing you typed later picks one. Keep one here, or correct it in a session.';
}

export const KEEP_TOOLTIP =
	'Keeps this statement. The other is superseded, and can be undone from Recent changes.';

export interface MeasureDef {
	key: keyof MeasureSeriesRow;
	label: string;
	tooltip: string;
	better?: TrendDirection;
	format: (n: number) => string;
	formatDelta: (d: number) => string;
}

const count = (n: number): string => Math.round(n).toLocaleString('en-US');
const share = (n: number): string => `${(n * 100).toFixed(1)}%`;
const decimal = (n: number): string => n.toFixed(2);

export const MEASURES: MeasureDef[] = [
	{
		key: 'statements_current',
		label: 'Statements in force',
		tooltip:
			'Single instructions or facts currently served, across lessons and files. New guidance raises it; covered and merged statements lower it.',
		better: 'lower',
		format: count,
		formatDelta: formatSignedCount
	},
	{
		key: 'subjects_shared',
		label: 'Subjects in 2+ items',
		tooltip:
			'Subjects whose statements sit in two or more lessons or files: the same thing said in more than one place.',
		better: 'lower',
		format: count,
		formatDelta: formatSignedCount
	},
	{
		key: 'conflicts_open',
		label: 'Open conflicts',
		tooltip:
			'Pairs of statements that tell a session opposite things and that no rule has settled.',
		better: 'lower',
		format: count,
		formatDelta: formatSignedCount
	},
	{
		key: 'guidance_chars',
		label: 'Characters of guidance',
		tooltip: 'Total length of the statements in force.',
		better: 'lower',
		format: count,
		formatDelta: formatSignedCount
	},
	{
		key: 'subjects',
		label: 'Subjects',
		tooltip: 'Groups of statements about the same thing.',
		format: count,
		formatDelta: formatSignedCount
	},
	{
		key: 'misgrouped_share',
		label: 'Misgrouped',
		tooltip:
			'Share of judged statements the judge said did not belong in their subject. High means grouping is too loose.',
		better: 'lower',
		format: share,
		formatDelta: formatSignedPoints
	},
	{
		key: 'split_subject_share',
		label: 'Found outside its subject',
		tooltip:
			"Share of covered statements the judge found among a subject's neighbours instead of inside it. High means grouping is too tight.",
		better: 'lower',
		format: share,
		formatDelta: formatSignedPoints
	},
	{
		key: 'grouping_distance',
		label: 'Grouping threshold',
		tooltip:
			"How close a statement must be to a subject's centre to join it. The worker moves it one step each run towards whichever of the two shares above is larger.",
		format: decimal,
		formatDelta: formatSignedDecimal
	}
];

export type SectionState =
	| 'off-never'
	| 'off-history'
	| 'first-pass'
	| 'steady'
	| 'degraded'
	| 'inert'
	| 'empty'
	| 'unknown';

export function sectionState(
	capability: CapabilityRow | null,
	measures: MeasureSeriesRow[]
): SectionState {
	if (!capability) return 'unknown';
	if (capability.state === 'off') return measures.length === 0 ? 'off-never' : 'off-history';
	if (capability.state === 'degraded') return 'degraded';
	if (capability.state === 'inert') return 'inert';
	if (measures.length === 0) return 'first-pass';
	const newest = measures[measures.length - 1]!;
	if (newest.statements_current === 0 && /nothing waiting/i.test(capability.detail)) {
		return 'empty';
	}
	return 'steady';
}

/** Bands 2 to 5 render for every state except these. */
export function showsBands(state: SectionState): boolean {
	return state !== 'off-never' && state !== 'empty';
}

export const COPY = {
	lede: "Every change consolidation made to this project's lessons and guidance files, and its figures by run.",
	offNever:
		'New and changed lessons and guidance files are not compared with anything. Turning it on sends their text to the consolidation model.',
	offHistory: 'The figures and changes below are from its last run.',
	firstPass:
		"Working through this project's existing guidance for the first time. The figures appear when that run finishes.",
	empty:
		'This project has no lessons or guidance files yet. Consolidation starts when a lesson is recorded here or a memory, rule, agent, skill or instruction file in it is indexed.',
	unknown: 'This mike does not report the consolidation worker.'
};

export function formatClock(iso: string | null): string {
	if (!iso) return '—';
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return '—';
	return date.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
}

export function formatDayMonth(iso: string): string {
	return new Date(iso).toLocaleDateString('en-GB', { day: 'numeric', month: 'short' });
}

export function isToday(iso: string, now: number = Date.now()): boolean {
	return new Date(iso).toDateString() === new Date(now).toDateString();
}

/** `HH:MM` today, `d Mon` otherwise. */
export function formatWhen(iso: string | null, now: number = Date.now()): string {
	if (!iso) return '—';
	return isToday(iso, now) ? formatClock(iso) : formatDayMonth(iso);
}

export function formatFull(iso: string | null): string {
	if (!iso) return '';
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return '';
	return `${formatDayMonth(iso)} ${date.getFullYear()} ${formatClock(iso)}`;
}
