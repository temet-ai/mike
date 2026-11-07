import { describe, expect, it } from 'vitest';
import { normalizeFlowDataset } from './projection';
import { FLOW_SESSION_ID, flowActivity, flowActor } from './__fixtures__/sessionFlow';
import { buildFlowRequestFilter, mainSessionActorId, requestIdsForActorIds } from './requestFilter';

function baseDataset() {
	const coordinatorId = `session:${FLOW_SESSION_ID}`;
	const dispatchId = 'dispatch:1';
	const workerId = 'worker:1';
	const toolActorId = 'tool:bash';

	return normalizeFlowDataset({
		sessionId: FLOW_SESSION_ID,
		actors: [
			flowActor(coordinatorId, { kind: 'session', label: 'Session', producer_id: null }),
			flowActor(dispatchId, {
				kind: 'unresolved_dispatch',
				label: 'Dispatch 1',
				producer_id: null
			}),
			flowActor(workerId, {
				kind: 'worker',
				label: 'Agent 1',
				parent_actor_id: dispatchId,
				producer_id: workerId
			}),
			flowActor(toolActorId, { kind: 'tool_service', label: 'Bash', producer_id: null })
		],
		activities: [
			flowActivity('requests:100:started', {
				actor_id: coordinatorId,
				source: {
					table: 'requests',
					row_id: '100',
					session_id: FLOW_SESSION_ID,
					request_id: '100',
					detail_href: '/api/requests/100'
				}
			}),
			flowActivity('dispatches:1:observed', {
				actor_id: dispatchId,
				kind: 'dispatches',
				lifecycle: 'observed',
				source: {
					table: 'agents.dispatches',
					row_id: 'd1',
					session_id: FLOW_SESSION_ID,
					request_id: '100',
					detail_href: null
				}
			}),
			flowActivity('requests:201:started', {
				actor_id: workerId,
				source: {
					table: 'requests',
					row_id: '201',
					session_id: FLOW_SESSION_ID,
					request_id: '201',
					detail_href: '/api/requests/201'
				}
			}),
			flowActivity('requests:202:started', {
				actor_id: workerId,
				source: {
					table: 'requests',
					row_id: '202',
					session_id: FLOW_SESSION_ID,
					request_id: '202',
					detail_href: '/api/requests/202'
				}
			})
		]
	});
}

describe('mainSessionActorId', () => {
	it('finds the coordinator session actor for the dataset session id', () => {
		const dataset = baseDataset();
		expect(mainSessionActorId(dataset)).toBe(`session:${FLOW_SESSION_ID}`);
	});
});

describe('requestIdsForActorIds', () => {
	it('collects request ids only from "requests" kind activities on the given actors', () => {
		const dataset = baseDataset();
		expect(requestIdsForActorIds(dataset, ['worker:1'])).toEqual(new Set([201, 202]));
	});

	it('returns an empty set for an actor with no request activities', () => {
		const dataset = baseDataset();
		expect(requestIdsForActorIds(dataset, ['dispatch:1'])).toEqual(new Set());
	});
});

describe('buildFlowRequestFilter', () => {
	it('returns null when nothing is selected', () => {
		expect(buildFlowRequestFilter(baseDataset(), null)).toBeNull();
	});

	it('filters to the worker actor bundle for a subagent selection', () => {
		const filter = buildFlowRequestFilter(baseDataset(), { kind: 'actor', id: 'worker:1' });
		expect(filter).not.toBeNull();
		expect(filter!.label).toBe('Agent 1');
		expect(filter!.requestIds).toEqual(new Set([201, 202]));
	});

	it('resolves the same bundle whether the dispatch or the worker actor id is selected', () => {
		const dataset = baseDataset();
		const fromDispatch = buildFlowRequestFilter(dataset, { kind: 'actor', id: 'dispatch:1' });
		const fromWorker = buildFlowRequestFilter(dataset, { kind: 'actor', id: 'worker:1' });
		expect(fromDispatch!.requestIds).toEqual(fromWorker!.requestIds);
	});

	it('filters to the main session for the coordinator actor', () => {
		const dataset = baseDataset();
		const filter = buildFlowRequestFilter(dataset, {
			kind: 'actor',
			id: `session:${FLOW_SESSION_ID}`
		});
		expect(filter).toEqual({ label: 'main session', requestIds: new Set([100]) });
	});

	it('returns null for an actor that is neither the coordinator nor part of an agent identity', () => {
		const filter = buildFlowRequestFilter(baseDataset(), { kind: 'actor', id: 'tool:bash' });
		expect(filter).toBeNull();
	});

	it('filters to a single request for an activity selection', () => {
		const filter = buildFlowRequestFilter(baseDataset(), {
			kind: 'activity',
			id: 'requests:201:started'
		});
		expect(filter).toEqual({ label: 'Agent 1 · requests', requestIds: new Set([201]) });
	});

	it('returns null for an activity with no request id', () => {
		const dataset = normalizeFlowDataset({
			sessionId: FLOW_SESSION_ID,
			actors: [flowActor('worker-a')],
			activities: [
				flowActivity('messages:m1:queued', {
					kind: 'messages',
					source: {
						table: 'session_messages',
						row_id: 'm1',
						session_id: FLOW_SESSION_ID,
						request_id: null,
						detail_href: null
					}
				})
			]
		});
		expect(
			buildFlowRequestFilter(dataset, { kind: 'activity', id: 'messages:m1:queued' })
		).toBeNull();
	});
});
