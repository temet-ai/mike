import { ApiError } from '$lib/api';
import { resolve } from '$app/paths';
import { guardQuestionLabel, guardQuestionShortLabel, labelGuardWake } from '$lib/api/improvement';
import { shortSessionId } from '$lib/format';
import { operatorPromptText } from '$lib/sessionPrompt';
import type { GuardWake, GuardWakeOutcome } from '$lib/api/improvement';
import { projectLabelOf } from '$lib/components/improvement/groupByConversation';

/**
 * Client for the turn guard's evidence surface: GET /api/steering/guard/effect (the honest
 * holdback-vs-treated effect estimate per question, api/steering_guard_effect.rs +
 * steerer/guard/promotion/effect.rs) and GET /api/improvement/guard (delivered wakes/appends,
 * newest first, store/persistence/steering/promotion/interventions_feed.rs). PATCH
 * /api/steering/evaluations/{uuid}/label (the operator's "was this right?" verdict) is
 * `labelGuardWake`, re-exported from `$lib/api/improvement` — one canonical implementation,
 * since both this page and the "Waiting for you" grid call the same endpoint.
 *
 * Per the coordinator's ruling (2026-09-29): a guard question is measured ONLY by its
 * randomised holdback-vs-treated arm comparison (clustered-bootstrap risk difference, read
 * point 34 holdback events — `promotion::effect::READ_POINT_HOLDBACK`), never a before/after
 * corrected share. `GuardEffectEstimate` below mirrors the real `EffectEstimate`/`Estimate`/
 * `ArmSummary` Rust types exactly — it is NOT the same `EffectResult` shape lessons/directives
 * use (that type's before/after language doesn't apply here at all).
 *
 * `GuardWake` and `labelGuardWake` are re-exported from `$lib/api/improvement` (P6's client):
 * `GuardWakeCard`/`GuardWakeList` render wakes fetched by either client (unfiltered per
 * question here, filtered to unlabelled there), so both need the exact same view shape.
 */
export type { GuardWake, GuardWakeOutcome };
export { labelGuardWake, labelGuardWake as labelGuardEvaluation };

/**
 * The turn-guard questions this install has configured, with the plain-language text each one
 * asks. There is no server-side label for a `Question` (`crates/mike-features/src/steerer/guard/mod.rs`
 * only exposes `code()` — a wire key, never display text), so this is a small, deliberately
 * static map over the closed 4-value enum rather than a fabricated per-request field. Only
 * `relapse` and `unobserved_done` are live on this install (PLAN.md §2.6); the other two codes
 * fall back to a humanized key so an unexpected question never renders blank.
 */
export { guardQuestionLabel, guardQuestionShortLabel };

export interface ArmSummary {
	n: number;
	bad: number;
	pBad: number | null;
}

export interface GuardEstimate {
	treated: ArmSummary;
	holdback: ArmSummary;
	rd: number | null;
	ci: [number, number] | null;
}

export type GuardEffectStatus = 'readable' | 'monitoring';

export interface GuardEffectEstimate {
	questionKey: string | null;
	asOfSecs: number;
	primary: GuardEstimate;
	secondary: GuardEstimate;
	perProtocolNotRandomised: GuardEstimate;
	missing: Record<string, number>;
	events: number;
	status: GuardEffectStatus;
}

export interface GuardEffectResponse {
	asOfSecs: number;
	readPointHoldback: number;
	questions: GuardEffectEstimate[];
}

interface ArmSummaryDto {
	n: number;
	bad: number;
	p_bad: number | null;
}

interface EstimateDto {
	treated: ArmSummaryDto;
	holdback: ArmSummaryDto;
	rd: number | null;
	ci: [number, number] | null;
}

interface EffectEstimateDto {
	question: string | null;
	as_of_secs: number;
	primary: EstimateDto;
	secondary: EstimateDto;
	per_protocol_not_randomised: EstimateDto;
	missing: Record<string, number>;
	events: number;
	status: GuardEffectStatus;
}

interface GuardEffectResponseDto {
	as_of_secs: number;
	read_point_holdback: number;
	questions: EffectEstimateDto[];
}

interface DeliveredWakeDto {
	uuid: string;
	session_uuid: string;
	session_id: number;
	operator_turn_id: number;
	question: string | null;
	arm: 'holdback' | 'treated' | null;
	dispatched_at: string;
	outcome_label: 'right' | 'wrong' | null;
	label_source: string | null;
	delivered_request_id: number | null;
	wake_state: string | null;
	outcome_reaction: string | null;
	/** Landing alongside session_title per P4 (2026-09-29): 'wake' | 'append'. */
	delivery_channel?: 'wake' | 'append' | null;
	/** Landing alongside delivery_channel per P4 (2026-09-29): sessions.sessions.title. */
	session_title?: string | null;
	/** The session's first operator prompt, the same fallback name the sessions list uses. */
	session_first_prompt?: string | null;
	project_path?: string | null;
	/** steering.interventions.payload_text: the note the guard added, verbatim. */
	note_text?: string | null;
}

interface ImprovementGuardResponseDto {
	interventions: DeliveredWakeDto[];
}

function toArmSummary(dto: ArmSummaryDto): ArmSummary {
	return { n: dto.n, bad: dto.bad, pBad: dto.p_bad };
}

function toEstimate(dto: EstimateDto): GuardEstimate {
	return {
		treated: toArmSummary(dto.treated),
		holdback: toArmSummary(dto.holdback),
		rd: dto.rd,
		ci: dto.ci
	};
}

function toGuardEffectEstimate(dto: EffectEstimateDto): GuardEffectEstimate {
	return {
		questionKey: dto.question,
		asOfSecs: dto.as_of_secs,
		primary: toEstimate(dto.primary),
		secondary: toEstimate(dto.secondary),
		perProtocolNotRandomised: toEstimate(dto.per_protocol_not_randomised),
		missing: dto.missing,
		events: dto.events,
		status: dto.status
	};
}

function sessionHrefFor(sessionUuid: string): string {
	return resolve(`sessions/${encodeURIComponent(sessionUuid)}` as `sessions/${string}`);
}

function turnHrefFor(sessionUuid: string, deliveredRequestId: number | null): string {
	if (deliveredRequestId === null) return sessionHrefFor(sessionUuid);
	return resolve(
		`sessions/${encodeURIComponent(sessionUuid)}/requests/${deliveredRequestId}` as `sessions/${string}/requests/${string}`
	);
}

/**
 * `outcome_reaction` comes straight off `memory.turn_verdicts.reaction` and carries values this
 * area doesn't otherwise use (e.g. a bare `null` for "no live verdict yet"); narrow it to the
 * outcomes GuardWakeCard actually renders text for, folding anything else into `unknown` rather
 * than passing an unrecognised string through.
 */
function toWakeOutcome(reaction: string | null): GuardWakeOutcome {
	switch (reaction) {
		case 'accepted':
		case 'redirected':
		case 'corrected':
		case 'closed_in_frustration':
		case 'no_response':
		case 'unclear':
			return reaction;
		default:
			return reaction === null || reaction === '' ? null : 'unknown';
	}
}

/**
 * `wake_state`/`delivery_channel` distinguish two different things: `delivery_channel` is the
 * mechanism (wake vs append — what `GuardWakeCard` needs), `wake_state` is that delivery's own
 * lifecycle (pending/delivering/pushed/...). Falls back to 'wake' when `delivery_channel` isn't
 * on the response yet (pre-P4-push rows) rather than guessing from `wake_state`, which can't
 * distinguish `CappedCorrection` (channel "append") from `Wake` (both leave `wake_state` set).
 */
function toGuardWake(dto: DeliveredWakeDto): GuardWake {
	return {
		id: dto.operator_turn_id,
		evaluationUuid: dto.uuid,
		questionKey: dto.question ?? '',
		questionLabel: guardQuestionLabel(dto.question ?? ''),
		arm: dto.delivery_channel ?? 'wake',
		sessionTitle: sessionTitleFor(dto),
		sessionHref: sessionHrefFor(dto.session_uuid),
		projectLabel: projectLabelOf(dto.project_path),
		projectPath: dto.project_path ?? null,
		turnHref: turnHrefFor(dto.session_uuid, dto.delivered_request_id),
		deliveredAt: dto.dispatched_at,
		outcome: toWakeOutcome(dto.outcome_reaction),
		operatorLabel: dto.outcome_label,
		noteText: dto.note_text ?? null
	};
}

/** Same headline rule as the sessions list (SessionRow): title, else first prompt, else short id. */
function sessionTitleFor(dto: DeliveredWakeDto): string {
	if (dto.session_title) return dto.session_title;
	const prompt = operatorPromptText(dto.session_first_prompt);
	return prompt.length > 0 ? prompt : shortSessionId(dto.session_uuid);
}

/**
 * Whether the next turn has a judged outcome, so the operator has something to weigh the wake
 * against. Mirrors `JUDGED_REACTIONS` in store/persistence/improvement/summary.rs, which counts
 * the same cards for the "Waiting for you" summary.
 */
export function isJudged(wake: GuardWake): boolean {
	return wake.outcome !== null && wake.outcome !== 'unknown';
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

export async function getGuardEffect(): Promise<GuardEffectResponse> {
	const dto = await send<GuardEffectResponseDto>('GET', '/api/steering/guard/effect');
	return {
		asOfSecs: dto.as_of_secs,
		readPointHoldback: dto.read_point_holdback,
		questions: dto.questions.map(toGuardEffectEstimate)
	};
}

/** Every delivered wake/append, newest first — the "Waiting for you" grid filters this client-side. */
export async function getDeliveredWakes(): Promise<GuardWake[]> {
	const dto = await send<ImprovementGuardResponseDto>('GET', '/api/improvement/guard');
	return collapseRepeats(dto.interventions.map(toGuardWake));
}

/**
 * Collapses the guard's repeats — the same question firing more than once in one turn — into
 * the newest card with the same verdict state, which carries the others' uuids so one verdict labels all of them.
 */
export function collapseRepeats(wakes: GuardWake[]): GuardWake[] {
	const byKey = new Map<string, GuardWake>();
	for (const wake of wakes) {
		const key = `${wake.id}:${wake.questionKey}:${wake.operatorLabel ?? ''}`;
		const first = byKey.get(key);
		if (first) {
			first.repeatUuids = [...(first.repeatUuids ?? []), wake.evaluationUuid];
		} else {
			byKey.set(key, { ...wake, repeatUuids: [] });
		}
	}
	return [...byKey.values()];
}

/** Every evaluation a card stands for: its own and its collapsed repeats. */
export function evaluationUuidsOf(wake: GuardWake): string[] {
	return [wake.evaluationUuid, ...(wake.repeatUuids ?? [])];
}

/** Buckets an already-fetched wake list by `questionKey`, for the guard page's one-panel-per-question layout. */
export function groupWakesByQuestion(wakes: GuardWake[]): Record<string, GuardWake[]> {
	const out: Record<string, GuardWake[]> = {};
	for (const wake of wakes) {
		(out[wake.questionKey] ??= []).push(wake);
	}
	return out;
}
