import { ApiError } from '$lib/api';
import { getLlmSpend } from '$lib/api/llmSpend';
import { projectLabelOf } from '$lib/components/improvement/groupByConversation';

/**
 * Client for the "What the judge did" area: lesson candidates the judge's corrections produced,
 * the unified action feed, the corrected-share trend, and the judge's own running cost.
 * See artifacts/2026-09-29/judgement-at-work/PLAN.md §2.4 and design-spec.md §4.
 */

export type LessonKind = 'correction' | 'convention' | 'warning' | 'preference';
export type LessonSource = 'judge' | 'directive';
export type RejectReason = 'not_a_correction' | 'too_specific' | 'wrong' | 'other';

export interface LessonCandidate {
	id: number;
	text: string;
	kind: LessonKind;
	source: LessonSource;
	evidenceCount: number;
	correctingQuote: string;
	createdAt: string;
	sessionTitle: string;
	sessionHref: string;
	turnHref: string;
	projectLabel: string;
	projectPath?: string | null;
	askText: string;
}

export interface LessonCandidatesResponse {
	candidates: LessonCandidate[];
}

export interface EvidenceCorrection {
	turnHref: string;
	sessionTitle: string;
	createdAt: string;
	correctingQuote: string;
	existingLessonText: string | null;
	existingLessonHref: string | null;
}

export type EffectResult =
	| { status: 'not_computed' }
	| { status: 'not_enough_data'; n: number; readPoint: number }
	| {
			status: 'measured';
			before: { sharePct: number; n: number; windowDays: number };
			after: { sharePct: number; n: number; windowDays: number };
			readPoint: number;
			ciLowPct: number;
			ciHighPct: number;
			verdict: 'fewer_corrections' | 'no_change_seen';
	  }
	| {
			/**
			 * A guard question's effect is never `not_enough_data`/`measured` — those describe a
			 * before/after share read, and a guard question is checked by randomised holdback
			 * comparison instead (PLAN §1.2), a different method with different evidence quality.
			 * `EffectVerdict.svelte` never handles this branch; `ActionEffectRow` renders it
			 * directly (ui-designer ruling, design-spec.md §4.8/§4.10, artifacts commit b75aff8).
			 */
			status: 'randomised';
			nHoldback: number;
			nTreated: number;
			readPoint: number;
			detailHref: string;
	  };

/**
 * The before/after measurement states of `EffectResult`, excluding `randomised` — a lesson's
 * or directive's effect is always one of these, never `randomised` (that shape belongs to a guard
 * question only). `EffectVerdict.svelte` takes this narrower type so the compiler enforces that
 * it never has to handle the `randomised` branch, matching the ui-designer ruling that
 * `ActionEffectRow` renders that branch itself and never hands it to `EffectVerdict`.
 */
export type MeasuredEffectResult = Exclude<EffectResult, { status: 'randomised' }>;

export interface LessonDetail {
	id: number;
	text: string;
	kind: LessonKind;
	source: LessonSource;
	stage: 'raw' | 'daily' | 'long_term' | 'archived';
	evidenceCount: number;
	createdAt: string;
	askText: string;
	assistantEndingText: string;
	correctingQuote: string;
	sessionTitle: string;
	sessionHref: string;
	turnHref: string;
	judgeRationale: string;
	similarCorrections: EvidenceCorrection[];
	reinforcingCorrections: EvidenceCorrection[];
	injectionCount: number | null;
	wentLiveAt: string | null;
	effect: MeasuredEffectResult | null;
}

export interface ActionEffectItem {
	kind: 'lesson' | 'directive' | 'guard_question';
	id: number | string;
	label: string;
	scope: string;
	/** `null` when the wire row carries no timestamp for this action (guard questions today). */
	decidedAt: string | null;
	href: string;
	effect: EffectResult;
}

export interface ActionsFeedResponse {
	actions: ActionEffectItem[];
}

/**
 * One week of the corrected share, counted exactly as the Judge page's label alarm counts it
 * (`eval_metrics/sql_label_mix.rs`): every real turn the judge labelled in the 7 days before
 * `weekEnd` (midnight UTC), all projects, replied or not.
 */
export interface TrendPoint {
	weekEnd: string;
	k: number;
	n: number;
	/** The judges that wrote this week's labels, most labels first, in plain words. */
	judges: TrendJudge[];
	/** Main judges (≥10% of the week's labels) that were not main judges the labelled week before. */
	judgesStarted: string[];
	/** Main judges of the labelled week before that are no longer main judges this week. */
	judgesStopped: string[];
}

export interface TrendJudge {
	name: string;
	count: number;
}

export interface TrendMarker {
	date: string;
	label: string;
	href: string;
}

export interface TrendResponse {
	weekly: TrendPoint[];
	markers: TrendMarker[];
}

export interface ImprovementSummary {
	lessonCandidateCount: number;
	openProposalCount: number;
	unlabelledWakeCount: number;
	week: {
		lessonsApproved: number;
		lessonsRejected: number;
		wakes: number;
		appends: number;
		proposalsDecided: number;
		proposalsApproved: number;
		proposalsRejected: number;
	};
}

/** The wake shape the "Waiting for you" grid and the guard page both render (design-spec.md §4.5–4.6). */
export type GuardWakeOutcome =
	| 'accepted'
	| 'redirected'
	| 'corrected'
	| 'closed_in_frustration'
	| 'no_response'
	| 'unclear'
	| 'unknown'
	| null;

export interface GuardWake {
	id: number;
	/** The `steering.evaluations` row this wake/append came from — what the label POST addresses. */
	evaluationUuid: string;
	questionKey: string;
	questionLabel: string;
	arm: 'wake' | 'append';
	sessionTitle: string;
	sessionHref: string;
	projectLabel: string;
	projectPath: string | null;
	turnHref: string;
	deliveredAt: string;
	outcome: GuardWakeOutcome;
	operatorLabel: 'right' | 'wrong' | null;
	/** The note the guard added (`steering.interventions.payload_text`), verbatim. */
	noteText?: string | null;
	/** Earlier evaluations of the same question in the same turn, shown as this one card. */
	repeatUuids?: string[];
}

async function send<T>(method: string, path: string, body?: unknown): Promise<T> {
	const res = await fetch(path, {
		method,
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `${method} ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const parsed = (await res.json()) as { error?: string };
			if (parsed.error) message = parsed.error;
		} catch {
			message = `${method} ${path} failed: ${res.status}`;
		}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as T;
}

// --- Wire DTOs (snake_case, as returned by crates/mike/src/api/improvement/*.rs) and their
// mapping into the camelCase view types above. Confirmed against P1/P2/P3's own field lists in
// the run thread for fleet-judgement-at-work-541ce7e3 (2026-09-28); still-missing fields on the
// lesson-detail row (judge rationale, similar/reinforcing corrections, injection count, went-live
// date, effect) are called out at getLessonDetail below and render as this area's existing empty
// states, not as fabricated data.

interface LessonCandidateRowDto {
	id: number;
	project_id: number | null;
	project_path: string | null;
	kind: LessonKind;
	text: string;
	source: LessonSource;
	stage: string;
	source_turn_id: number | null;
	created_at: string;
	evidence_count: number;
	session_uuid: string | null;
	session_title: string | null;
	turn_closed_at: string | null;
	user_prompt: string | null;
	final_assistant_text: string | null;
	correcting_message: string | null;
}

function sessionHrefFor(sessionUuid: string | null): string {
	return sessionUuid ? `/sessions/${encodeURIComponent(sessionUuid)}` : '#';
}

function turnHrefFor(sessionUuid: string | null, turnId: number | null): string {
	const base = sessionHrefFor(sessionUuid);
	return sessionUuid && turnId !== null ? `${base}?request=${turnId}` : base;
}

function toLessonCandidate(dto: LessonCandidateRowDto): LessonCandidate {
	return {
		id: dto.id,
		text: dto.text,
		kind: dto.kind,
		source: dto.source,
		evidenceCount: dto.evidence_count,
		correctingQuote: dto.correcting_message ?? '(the correcting message is no longer available)',
		createdAt: dto.created_at,
		sessionTitle: dto.session_title ?? 'an earlier session',
		sessionHref: sessionHrefFor(dto.session_uuid),
		turnHref: turnHrefFor(dto.session_uuid, dto.source_turn_id),
		projectLabel: projectLabelOf(dto.project_path),
		projectPath: dto.project_path,
		askText: dto.user_prompt ?? ''
	};
}

export async function listLessonCandidates(): Promise<LessonCandidatesResponse> {
	const dto = await send<{ candidates: LessonCandidateRowDto[] }>(
		'GET',
		'/api/improvement/lessons'
	);
	return { candidates: dto.candidates.map(toLessonCandidate) };
}

const LESSON_STAGES = new Set(['raw', 'daily', 'long_term', 'archived']);

function toLessonStage(stage: string): LessonDetail['stage'] {
	return LESSON_STAGES.has(stage) ? (stage as LessonDetail['stage']) : 'raw';
}

/**
 * `origin_*` fields are only present once `GET /api/improvement/lessons/{id}`'s read-time join
 * (store/persistence/lessons/detail.rs::list_lesson_evidence) has resolved them — a missing or
 * deleted session/turn leaves them null, never assume non-null (P3, land/p3-36d37c30).
 */
interface SimilarCorrectionDto {
	item_id: number;
	similarity: number;
	text: string;
	origin_session_id: number | null;
	origin_turn_id: number | null;
	origin_session_uuid: string | null;
	origin_session_title: string | null;
	origin_turn_closed_at: string | null;
}

interface LessonEvidenceRowDto {
	id: number;
	verdict_id: number;
	operator_turn_id: number;
	reason: string;
	similar: SimilarCorrectionDto[];
	model: string;
	created_at: string;
}

interface LessonInjectionSummaryDto {
	count: number;
	first_injected_at: string | null;
	last_injected_at: string | null;
}

interface LessonDetailDto extends LessonCandidateRowDto {
	reinforcements: LessonEvidenceRowDto[];
	injections: LessonInjectionSummaryDto;
	went_live_at: string | null;
	effect: EffectViewDto | null;
}

/**
 * `origin_session_uuid`/`origin_turn_id`/`origin_session_title`/`origin_turn_closed_at` are all
 * independently nullable (P3: a missing/deleted session or turn leaves them null) — each falls
 * back honestly (an unlinked "an earlier session", no date) rather than a fabricated link.
 * `existingLessonText`/`existingLessonHref` aren't in this row at all yet (no lesson-lookup join),
 * so those stay null; not fabricating a "this is already a lesson" claim the API can't back.
 */
function toSimilarCorrection(dto: SimilarCorrectionDto): EvidenceCorrection {
	return {
		turnHref: turnHrefFor(dto.origin_session_uuid, dto.origin_turn_id),
		sessionTitle: dto.origin_session_title ?? 'an earlier session',
		createdAt: dto.origin_turn_closed_at ?? '',
		correctingQuote: dto.text,
		existingLessonText: null,
		existingLessonHref: null
	};
}

/**
 * `GET /api/improvement/lessons/{id}` returns `{"candidate": LessonDetailDto}` (the row plus
 * `reinforcements`/`injections`/`went_live_at`/`effect`, landed in the run thread for
 * fleet-judgement-at-work-541ce7e3, 2026-09-28). `reinforcements` is every evidence row for this
 * lesson, newest first; the oldest (its founding correction) supplies the judge's rationale and
 * the "similar earlier corrections" it found at proposal time. Each reinforcement's own `reason`
 * is the proposer's generalisation text for THAT correction, not the operator's quote, so
 * `reinforcingCorrections` stays empty rather than mislabel it as one — the same honest-gap
 * choice as `similarCorrections`' missing session context. The row's own query still only matches
 * stage `raw`, so this 404s for an already-decided lesson today.
 */
export async function getLessonDetail(id: number): Promise<LessonDetail> {
	const dto = await send<{ candidate: LessonDetailDto }>('GET', `/api/improvement/lessons/${id}`);
	const c = dto.candidate;
	const founding = c.reinforcements.at(-1) ?? null;
	return {
		id: c.id,
		text: c.text,
		kind: c.kind,
		source: c.source,
		stage: toLessonStage(c.stage),
		evidenceCount: c.evidence_count,
		createdAt: c.created_at,
		askText: c.user_prompt ?? '(not captured)',
		assistantEndingText: c.final_assistant_text ?? '(not captured)',
		correctingQuote: c.correcting_message ?? '(the correcting message is no longer available)',
		sessionTitle: c.session_title ?? 'an earlier session',
		sessionHref: sessionHrefFor(c.session_uuid),
		turnHref: turnHrefFor(c.session_uuid, c.source_turn_id),
		judgeRationale: founding?.reason ?? 'Not available yet.',
		similarCorrections: (founding?.similar ?? []).map(toSimilarCorrection),
		reinforcingCorrections: [],
		injectionCount: c.injections.count,
		wentLiveAt: c.went_live_at,
		effect: c.effect ? toEffectResult(c.effect) : null
	};
}

export function approveLessonCandidate(id: number, text?: string): Promise<unknown> {
	return send('POST', `/api/improvement/lessons/${id}/approve`, text === undefined ? {} : { text });
}

export function rejectLessonCandidate(
	id: number,
	reason: RejectReason,
	note?: string
): Promise<unknown> {
	return send('POST', `/api/improvement/lessons/${id}/reject`, { reason, note });
}

interface PeriodStatsDto {
	label: string;
	k: number;
	n: number;
	share: number;
	window_start: string | null;
	window_end: string | null;
}

/** A lesson row's effect only — see `GuardEffectViewDto` for a guard question's own shape. */
type EffectViewDto =
	| { status: 'not_computed' }
	| { status: 'not_enough_data'; n_before: number; n_after: number; read_point: number }
	| {
			status: 'ready';
			read_point?: number;
			diff: number;
			ci_low: number;
			ci_high: number;
			before: PeriodStatsDto;
			after: PeriodStatsDto;
	  };

/**
 * A guard question's effect on the actions feed is never `not_enough_data`/`ready` — it is
 * always this one `randomised` shape (checked by holdback comparison, not a before/after share;
 * P2/P4, 2026-09-29, final field names in land/p2-6f856f1e). `measure_status` ("readable" once
 * P4's estimate has enough rows, "monitoring" otherwise — never an error for the whole feed) and
 * the nullable `rd`/`ci_low`/`ci_high` carry the real holdback-vs-treated estimate, but the view
 * mapping below still never renders them inline: the ui-designer ruling's copy already reads
 * correctly regardless of readiness ("collected so far ... see the full read on the guard page"),
 * and showing a second, differently-worded estimate here risks disagreeing with the guard page's
 * own (P4's `GET /api/steering/guard/effect`) — one surface owns the real number.
 */
interface GuardEffectViewDto {
	status: 'randomised';
	measure: 'holdback_estimate';
	measure_status: string;
	n_holdback: number;
	n_treated: number;
	read_point: number;
	rd: number | null;
	ci_low: number | null;
	ci_high: number | null;
	detail_endpoint: string;
}

/** `interval.rs::READ_POINT_N` — not yet echoed on the `ready` branch of the wire response. */
const FALLBACK_READ_POINT = 20;
/** `improvement/effect.rs::EFFECT_WINDOW_DAYS` — used only when a period's own window is null. */
const FALLBACK_WINDOW_DAYS = 14;

function windowDaysOf(period: PeriodStatsDto): number {
	if (!period.window_start || !period.window_end) return FALLBACK_WINDOW_DAYS;
	const start = Date.parse(period.window_start);
	const end = Date.parse(period.window_end);
	if (Number.isNaN(start) || Number.isNaN(end)) return FALLBACK_WINDOW_DAYS;
	return Math.max(1, Math.round((end - start) / 86_400_000));
}

function toEffectResult(dto: EffectViewDto): MeasuredEffectResult {
	if (dto.status === 'not_computed') return { status: 'not_computed' };
	if (dto.status === 'not_enough_data') {
		return {
			status: 'not_enough_data',
			n: Math.min(dto.n_before, dto.n_after),
			readPoint: dto.read_point
		};
	}
	const before = {
		sharePct: Math.round(dto.before.share * 100),
		n: dto.before.n,
		windowDays: windowDaysOf(dto.before)
	};
	const after = {
		sharePct: Math.round(dto.after.share * 100),
		n: dto.after.n,
		windowDays: windowDaysOf(dto.after)
	};
	return {
		status: 'measured',
		before,
		after,
		readPoint: dto.read_point ?? FALLBACK_READ_POINT,
		ciLowPct: Math.round(dto.ci_low * 100),
		ciHighPct: Math.round(dto.ci_high * 100),
		verdict: dto.ci_high < 0 ? 'fewer_corrections' : 'no_change_seen'
	};
}

type ActionFeedItemDto =
	| {
			kind: 'lesson';
			id: number;
			text: string;
			stage: string;
			source: string;
			created_at: string;
			project: string | null;
			effect: EffectViewDto;
	  }
	| { kind: 'guard_question'; id: string; mode: string; effect: GuardEffectViewDto };

const GUARD_QUESTION_LABELS: Record<string, string> = {
	relapse: 'Does this reply repeat a correction you already made?',
	unbacked_wait: 'Did the model wait on something it never backed up?',
	authorised_handback: 'Did the model hand back work it was authorised to keep going on?',
	unobserved_done: "Did the last turn's work actually ship?"
};

export function guardQuestionLabel(key: string): string {
	return GUARD_QUESTION_LABELS[key] ?? key.replaceAll('_', ' ');
}

const GUARD_QUESTION_SHORT: Record<string, string> = {
	relapse: 'Repeats a correction',
	unbacked_wait: 'Waits without a reason',
	authorised_handback: 'Hands back too early',
	unobserved_done: 'Done without checking'
};

/** A few words naming the question, for a list; the full question is its heading. */
export function guardQuestionShortLabel(key: string): string {
	return GUARD_QUESTION_SHORT[key] ?? guardQuestionLabel(key);
}

/**
 * `detailHref` is this app's own guard page, never P2's raw `detail_endpoint` API path
 * (ui-designer ruling) — not anchored to the specific question yet since the guard page doesn't
 * expose a per-question fragment target today.
 */
function toGuardEffectResult(dto: GuardEffectViewDto): EffectResult {
	return {
		status: 'randomised',
		nHoldback: dto.n_holdback,
		nTreated: dto.n_treated,
		readPoint: dto.read_point,
		detailHref: '/sessions/improvement/guard'
	};
}

function toActionEffectItem(dto: ActionFeedItemDto): ActionEffectItem {
	if (dto.kind === 'lesson') {
		return {
			kind: 'lesson',
			id: dto.id,
			label: `A lesson: ${dto.text}`,
			scope: dto.project ?? 'every project',
			decidedAt: dto.created_at,
			href: `/sessions/improvement/lessons/${dto.id}`,
			effect: toEffectResult(dto.effect)
		};
	}
	return {
		kind: 'guard_question',
		id: dto.id,
		label: `A guard question: ${guardQuestionLabel(dto.id)}`,
		scope: 'every project',
		decidedAt: null,
		href: '/sessions/improvement/guard',
		effect: toGuardEffectResult(dto.effect)
	};
}

export async function listActionEffects(
	params: { since?: string; project?: string } = {}
): Promise<ActionsFeedResponse> {
	const qs = new URLSearchParams();
	if (params.since) qs.set('since', params.since);
	if (params.project) qs.set('project', params.project);
	const query = qs.toString();
	const dto = await send<{ actions: ActionFeedItemDto[] }>(
		'GET',
		`/api/improvement/actions${query ? `?${query}` : ''}`
	);
	return { actions: dto.actions.map(toActionEffectItem) };
}

export interface TrendJudgeDto {
	label: string | null;
	bundle_id: number | null;
	count: number;
}

export interface TrendPointDto {
	week_end: string;
	k: number;
	n: number;
	judges: TrendJudgeDto[];
}

/** A judge that wrote at least this share of a week's labels counts as one of its main judges. */
const MAIN_JUDGE_SHARE = 0.1;

/** `bundle:14` → "configuration 14"; `legacy:<model>:<rubric>` → the model; unknown → null. */
export function judgeName(dto: TrendJudgeDto): string | null {
	if (dto.bundle_id !== null) return `configuration ${dto.bundle_id}`;
	const label = dto.label ?? '';
	if (label.startsWith('bundle:')) return `configuration ${label.replace('bundle:', '')}`;
	if (label.startsWith('legacy:')) {
		const model = label.split(':')[1] ?? 'none';
		return model === 'none' || model === '' ? null : model;
	}
	return label === '' ? null : label;
}

function judgesOf(dto: TrendPointDto): TrendJudge[] {
	const byName = new Map<string, number>();
	for (const j of dto.judges) {
		const name = judgeName(j);
		if (name !== null) byName.set(name, (byName.get(name) ?? 0) + j.count);
	}
	return [...byName.entries()]
		.map(([name, count]) => ({ name, count }))
		.sort((a, b) => b.count - a.count);
}

function mainJudges(judges: TrendJudge[], n: number): string[] {
	return judges.filter((j) => j.count / n >= MAIN_JUDGE_SHARE).map((j) => j.name);
}

export function toTrendPoints(dtos: TrendPointDto[]): TrendPoint[] {
	let previous: string[] | null = null;
	return dtos.map((dto) => {
		const judges = judgesOf(dto);
		const main = dto.n > 0 ? mainJudges(judges, dto.n) : null;
		const before = previous;
		const judgesStarted = main && before ? main.filter((j) => !before.includes(j)) : [];
		const judgesStopped = main && before ? before.filter((j) => !main.includes(j)) : [];
		if (main !== null) previous = main;
		return { weekEnd: dto.week_end, k: dto.k, n: dto.n, judges, judgesStarted, judgesStopped };
	});
}

interface ChangeLogRowDto {
	id: number;
	changed_at: string;
	subject_kind: string;
	subject_key: string;
	op: string;
	new_value: { stage?: string } | null;
}

function markerLabel(row: ChangeLogRowDto): string {
	const kind = row.subject_kind === 'lesson_decision' ? 'lesson' : 'directive';
	const stage = row.new_value?.stage;
	if (stage === 'daily' || stage === 'long_term') return `A ${kind} was approved`;
	if (stage === 'archived') return `A ${kind} was rejected`;
	return `A ${kind} was decided`;
}

function markerHref(row: ChangeLogRowDto): string {
	return row.subject_kind === 'lesson_decision'
		? `/sessions/improvement/lessons/${row.subject_key}`
		: '/sessions/improvement';
}

function toTrendMarker(row: ChangeLogRowDto): TrendMarker {
	return { date: row.changed_at, label: markerLabel(row), href: markerHref(row) };
}

export async function getCorrectionTrend(params: { weeks?: number } = {}): Promise<TrendResponse> {
	const qs = new URLSearchParams();
	qs.set('weeks', String(params.weeks ?? 8));
	const dto = await send<{ points: TrendPointDto[]; markers: ChangeLogRowDto[] }>(
		'GET',
		`/api/improvement/trend?${qs.toString()}`
	);
	return {
		weekly: toTrendPoints(dto.points),
		markers: dto.markers.map(toTrendMarker)
	};
}

/** One thing the operator said, in full, as captured. */
export interface RepeatItem {
	turn_id: number;
	session_id: string;
	at: string;
	project: string;
	text: string;
}

/** A group of messages that ask for the same thing. `weekly` lines up with `week_starts`. */
export interface RepeatGroup {
	id: number;
	n: number;
	first_at: string;
	last_at: string;
	projects: string[];
	weekly: number[];
	live: boolean;
	items: RepeatItem[];
}

export interface RepeatsResponse {
	weeks: number;
	threshold: number;
	min_count: number;
	turns: number;
	computed_at: string;
	week_starts: string[];
	groups: RepeatGroup[];
}

export async function getRepeats(params: { weeks?: number } = {}): Promise<RepeatsResponse> {
	const qs = new URLSearchParams();
	qs.set('weeks', String(params.weeks ?? 12));
	return send<RepeatsResponse>('GET', `/api/improvement/repeats?${qs.toString()}`);
}

interface SummaryViewDto {
	candidates_waiting: number;
	directive_proposals_waiting: number;
	guard_wakes_unlabelled: number;
	guard_wakes_7d: number;
	guard_appends_7d: number;
	lessons_approved_7d: number;
	lessons_rejected_7d: number;
	proposals_decided_7d: number;
	proposals_approved_7d: number;
	proposals_rejected_7d: number;
}

/**
 * The guard counts come from the same rows as `/api/improvement/guard` (the guard steerer's
 * delivered or pushed evaluations), so the "What happened" line and the wake list beside it agree.
 */
function toImprovementSummary(dto: SummaryViewDto): ImprovementSummary {
	return {
		lessonCandidateCount: dto.candidates_waiting,
		openProposalCount: dto.directive_proposals_waiting,
		unlabelledWakeCount: dto.guard_wakes_unlabelled,
		week: {
			lessonsApproved: dto.lessons_approved_7d,
			lessonsRejected: dto.lessons_rejected_7d,
			wakes: dto.guard_wakes_7d,
			appends: dto.guard_appends_7d,
			proposalsDecided: dto.proposals_decided_7d,
			proposalsApproved: dto.proposals_approved_7d,
			proposalsRejected: dto.proposals_rejected_7d
		}
	};
}

export async function getImprovementSummary(): Promise<ImprovementSummary> {
	const dto = await send<SummaryViewDto>('GET', '/api/improvement/summary');
	return toImprovementSummary(dto);
}

/**
 * The operator's "Was this right?" verdict on a delivered wake or append
 * (`POST /api/steering/evaluations/{uuid}/label`, keyed by the evaluation's own uuid, not a
 * numeric id). Re-exported as-is by `$lib/api/guardEffect.ts` (P7) alongside its own
 * `labelGuardEvaluation` — one canonical implementation of this endpoint kept here so the
 * "Waiting for you" grid and the guard page don't drift onto two different shapes; guard-wake
 * fetching (`getDeliveredWakes`) and question effects live in `guardEffect.ts` itself.
 */
export function labelGuardWake(evaluationUuid: string, label: 'right' | 'wrong'): Promise<unknown> {
	return send('POST', `/api/steering/evaluations/${encodeURIComponent(evaluationUuid)}/label`, {
		label
	});
}

export function triggerImprovementRollup(): Promise<unknown> {
	return send('POST', '/api/improvement/rollup');
}

export interface JudgeCost {
	weekUsd: number;
	weekCalls: number;
	monthUsd: number;
	ceilingUsd: number | null;
}

const JUDGE_ROLES = new Set(['judge_worker', 'session_judge']);

/**
 * What the judge itself costs — the by_role rows from the existing LLM spend ledger
 * (`/api/llm/spend`, `$lib/api/llmSpend.ts`), summed over `judge_worker` + `session_judge` for a
 * 7-day and a 30-day window. There is no monthly ceiling setting today (JC21: "the month-level
 * budget is a steerer question"), so `ceilingUsd` is always `null` — a real, resolved fact, not a
 * value this call failed to load.
 */
export async function getJudgeCost(): Promise<JudgeCost> {
	const since7 = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString();
	const since30 = new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString();
	const [week, month] = await Promise.all([
		getLlmSpend({ since: since7 }),
		getLlmSpend({ since: since30 })
	]);
	const sum = (rows: { key: string; cost_usd: number; call_count: number }[]) =>
		rows
			.filter((r) => JUDGE_ROLES.has(r.key))
			.reduce((acc, r) => ({ cost: acc.cost + r.cost_usd, calls: acc.calls + r.call_count }), {
				cost: 0,
				calls: 0
			});
	const weekSum = sum(week.by_role);
	const monthSum = sum(month.by_role);
	return {
		weekUsd: weekSum.cost,
		weekCalls: weekSum.calls,
		monthUsd: monthSum.cost,
		ceilingUsd: null
	};
}
