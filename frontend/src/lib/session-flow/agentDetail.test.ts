import { describe, expect, it } from 'vitest';
import { normalizeFlowDataset } from './projection';
import { FLOW_SESSION_ID, flowActivity, flowActor, flowTime } from './__fixtures__/sessionFlow';
import { agentModels, agentRequestSpan } from './agentDetail';

function requestActivity(
	id: string,
	overrides: Parameters<typeof flowActivity>[1] = {}
): ReturnType<typeof flowActivity> {
	return flowActivity(id, {
		actor_id: 'worker:1',
		source: {
			table: 'requests',
			row_id: id,
			session_id: FLOW_SESSION_ID,
			request_id: id,
			detail_href: `/api/requests/${id}`
		},
		...overrides
	});
}

describe('agentRequestSpan', () => {
	it('spans from the earliest start to the latest completion', () => {
		const dataset = normalizeFlowDataset({
			sessionId: FLOW_SESSION_ID,
			actors: [flowActor('worker:1')],
			activities: [
				requestActivity('201', { occurred_at: flowTime(0), ended_at: flowTime(1_000) }),
				requestActivity('202', { occurred_at: flowTime(2_000), ended_at: flowTime(5_000) })
			]
		});
		const span = agentRequestSpan(dataset, ['worker:1']);
		expect(span.startMs).toBe(Date.parse(flowTime(0)));
		expect(span.endMs).toBe(Date.parse(flowTime(5_000)));
		expect(span.open).toBe(false);
	});

	it('reports open when a request has started but not ended', () => {
		const dataset = normalizeFlowDataset({
			sessionId: FLOW_SESSION_ID,
			actors: [flowActor('worker:1')],
			activities: [requestActivity('201', { occurred_at: flowTime(0), ended_at: null })]
		});
		expect(agentRequestSpan(dataset, ['worker:1']).open).toBe(true);
	});

	it('ignores activities for actors outside the given set', () => {
		const dataset = normalizeFlowDataset({
			sessionId: FLOW_SESSION_ID,
			actors: [flowActor('worker:1'), flowActor('worker:2')],
			activities: [
				flowActivity('201', {
					actor_id: 'worker:2',
					source: {
						table: 'requests',
						row_id: '201',
						session_id: FLOW_SESSION_ID,
						request_id: '201',
						detail_href: null
					}
				})
			]
		});
		const span = agentRequestSpan(dataset, ['worker:1']);
		expect(span.startMs).toBeNull();
		expect(span.endMs).toBeNull();
		expect(span.open).toBe(false);
	});
});

describe('agentModels', () => {
	it('collects the distinct models used across an actor set', () => {
		const dataset = normalizeFlowDataset({
			sessionId: FLOW_SESSION_ID,
			actors: [flowActor('worker:1')],
			activities: [
				requestActivity('201', { metadata: { ...flowActivity().metadata, model: 'claude-opus-5' } }),
				requestActivity('202', { metadata: { ...flowActivity().metadata, model: 'claude-opus-5' } }),
				requestActivity('203', { metadata: { ...flowActivity().metadata, model: 'claude-haiku-5' } })
			]
		});
		expect(agentModels(dataset, ['worker:1'])).toEqual(['claude-haiku-5', 'claude-opus-5']);
	});

	it('returns an empty list when no request activity carries a model', () => {
		const dataset = normalizeFlowDataset({
			sessionId: FLOW_SESSION_ID,
			actors: [flowActor('worker:1')],
			activities: [requestActivity('201')]
		});
		expect(agentModels(dataset, ['worker:1'])).toEqual([]);
	});
});
