import { describe, expect, it } from 'vitest';
import type { SessionEvaluationRow, SessionSteeringRow } from '$lib/api/steering';
import { heldRowFrom, mergeTimeline, outcomePhrase } from './steeringTimeline';

function evaluation(over: Partial<SessionEvaluationRow> = {}): SessionEvaluationRow {
	return {
		id: 1,
		uuid: 'e-1',
		steerer_id: 1,
		steerer_name: 'turn-guard',
		steerer_kind: 'guard',
		operator_turn_id: 5,
		status: 'resolved',
		mode: 'live',
		question: 'unobserved_done',
		trigger_point: 'turn_end',
		trigger_request_id: null,
		bundle_id: null,
		intervention_uuid: null,
		dispatched_at: '2026-11-14T10:00:00Z',
		resolved_at: null,
		verdict_json: { precheck: { verdict: { send: false, score: 0.05, why: 'held' } } },
		confidence: null,
		reason: null,
		outcome_label: null,
		label_source: null,
		labelled_at: null,
		label_evidence: null,
		...over
	};
}

function row(
	id: number,
	created: string,
	evaluationUuid: string | null = null
): SessionSteeringRow {
	return {
		...heldRowFrom(evaluation(), 's'),
		intervention_id: id,
		created_at: created,
		evaluation_uuid: evaluationUuid
	};
}

describe('steering timeline', () => {
	it('adds a held evaluation that has no intervention', () => {
		const merged = mergeTimeline([row(10, '2026-11-14T09:00:00Z')], [evaluation()], 's');
		expect(merged.map((e) => e.key)).toEqual(['e1', 'i10']);
		expect(merged[0]?.held).toBe(true);
	});

	it('joins an evaluation to its intervention instead of adding a row', () => {
		const merged = mergeTimeline([row(10, '2026-11-14T10:00:00Z', 'e-1')], [evaluation()], 's');
		expect(merged).toHaveLength(1);
		expect(merged[0]?.held).toBe(true);
	});

	it('leaves out evaluations that were sent', () => {
		const sent = evaluation({
			verdict_json: { precheck: { verdict: { send: true, score: 0.9, why: '' } } }
		});
		expect(mergeTimeline([], [sent], 's')).toEqual([]);
	});

	it('orders newest first and breaks ties by id', () => {
		const merged = mergeTimeline(
			[row(1, '2026-11-14T10:00:00Z'), row(2, '2026-11-14T10:00:00Z')],
			[],
			's'
		);
		expect(merged.map((e) => e.row.intervention_id)).toEqual([2, 1]);
	});

	it('words the outcome from the point of view of the decision', () => {
		expect(outcomePhrase(evaluation({ outcome_label: 'right' }), false)?.text).toBe('sent right');
		expect(outcomePhrase(evaluation({ outcome_label: 'wrong' }), false)?.tone).toBe('wrong');
		expect(outcomePhrase(evaluation({ outcome_label: 'wrong' }), true)).toEqual({
			text: 'hold correct',
			tone: 'correct'
		});
		expect(outcomePhrase(evaluation({ outcome_label: 'right' }), true)?.text).toBe('hold wrong');
		expect(outcomePhrase(evaluation(), true)).toBeNull();
	});
});
