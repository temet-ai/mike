import {
	isHeldEvaluation,
	type SessionEvaluationRow,
	type SessionSteeringRow
} from '$lib/api/steering';

export const PRECHECK_QUESTION = 'unobserved_done';

export interface TimelineEntry {
	key: string;
	row: SessionSteeringRow;
	evaluation: SessionEvaluationRow | null;
	held: boolean;
	at: string;
	order: number;
}

export type OutcomeTone = 'correct' | 'wrong';

export interface OutcomePhrase {
	text: string;
	tone: OutcomeTone;
}

export const OUTCOME_TIP =
	'The labeller judged the nudge right or wrong. For a held nudge, a wrong label means the hold was correct.';

export function outcomePhrase(
	evaluation: SessionEvaluationRow | null,
	held: boolean
): OutcomePhrase | null {
	const label = evaluation?.outcome_label ?? null;
	if (label !== 'right' && label !== 'wrong') return null;
	if (held) {
		return label === 'wrong'
			? { text: 'hold correct', tone: 'correct' }
			: { text: 'hold wrong', tone: 'wrong' };
	}
	return label === 'right'
		? { text: 'sent right', tone: 'correct' }
		: { text: 'sent wrong', tone: 'wrong' };
}

export function heldRowFrom(
	evaluation: SessionEvaluationRow,
	sessionId: string
): SessionSteeringRow {
	return {
		intervention_id: -evaluation.id,
		session_id: sessionId,
		session_uuid: sessionId,
		project_path: null,
		steerer_name: evaluation.steerer_name,
		steerer_display_name: null,
		verdict: null,
		authority: null,
		mode: null,
		trigger_request_id: evaluation.trigger_request_id,
		sent_request_id: null,
		sent_response_id: null,
		operator_turn_id: evaluation.operator_turn_id,
		gate_reason: evaluation.reason,
		off_thread_topic: null,
		confidence: evaluation.confidence,
		anchor_text: null,
		verdict_json: evaluation.verdict_json,
		payload_text: null,
		steerer_model: null,
		sent_at: null,
		processed_at: evaluation.resolved_at,
		expires_at: null,
		reverted_at: null,
		revert_reason: null,
		created_at: evaluation.dispatched_at,
		evaluation_uuid: evaluation.uuid
	};
}

function newestFirst(a: TimelineEntry, b: TimelineEntry): number {
	if (a.at !== b.at) return a.at < b.at ? 1 : -1;
	return b.order - a.order;
}

export function mergeTimeline(
	rows: readonly SessionSteeringRow[],
	evaluations: readonly SessionEvaluationRow[],
	sessionId: string
): TimelineEntry[] {
	const byUuid = new Map(evaluations.map((e) => [e.uuid, e]));
	const joined = new Set<string>();
	const entries: TimelineEntry[] = rows.map((row) => {
		const evaluation = row.evaluation_uuid ? (byUuid.get(row.evaluation_uuid) ?? null) : null;
		if (evaluation) joined.add(evaluation.uuid);
		return {
			key: `i${row.intervention_id}`,
			row,
			evaluation,
			held: evaluation !== null && isHeldEvaluation(evaluation),
			at: row.created_at,
			order: row.intervention_id
		};
	});
	for (const evaluation of evaluations) {
		if (joined.has(evaluation.uuid)) continue;
		if (evaluation.intervention_uuid !== null || !isHeldEvaluation(evaluation)) continue;
		entries.push({
			key: `e${evaluation.id}`,
			row: heldRowFrom(evaluation, sessionId),
			evaluation,
			held: true,
			at: evaluation.dispatched_at,
			order: evaluation.id
		});
	}
	return entries.sort(newestFirst);
}
