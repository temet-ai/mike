import { describe, expect, it } from 'vitest';
import { normalizeFlowDataset } from './projection';
import { flowActivity, flowActor } from './__fixtures__/sessionFlow';
import { activityIdForRequest, buildRequestActivityIndex } from './requestActivation';

describe('buildRequestActivityIndex', () => {
	it('maps each request id to its recording activity', () => {
		const dataset = normalizeFlowDataset({
			sessionId: 'sid',
			actors: [flowActor('worker-a')],
			activities: [
				flowActivity('requests:201:started', {
					source: {
						table: 'requests',
						row_id: '201',
						session_id: 'sid',
						request_id: '201',
						detail_href: '/api/requests/201'
					}
				}),
				flowActivity('requests:207:started', {
					source: {
						table: 'requests',
						row_id: '207',
						session_id: 'sid',
						request_id: '207',
						detail_href: '/api/requests/207'
					}
				})
			]
		});
		const index = buildRequestActivityIndex(dataset);
		expect(activityIdForRequest(index, 201)).toBe('requests:201:started');
		expect(activityIdForRequest(index, 207)).toBe('requests:207:started');
	});

	it('returns null for a request the dataset has no activity for', () => {
		const dataset = normalizeFlowDataset({ sessionId: 'sid' });
		const index = buildRequestActivityIndex(dataset);
		expect(activityIdForRequest(index, 999)).toBeNull();
	});

	it('keeps the first activity when a request backs more than one (tool-loop continuation)', () => {
		const dataset = normalizeFlowDataset({
			sessionId: 'sid',
			actors: [flowActor('worker-a')],
			activities: [
				flowActivity('requests:201:started', {
					source: {
						table: 'requests',
						row_id: '201',
						session_id: 'sid',
						request_id: '201',
						detail_href: '/api/requests/201'
					}
				}),
				flowActivity('tools:201:call', {
					kind: 'tools',
					source: {
						table: 'tool_uses',
						row_id: 't1',
						session_id: 'sid',
						request_id: '201',
						detail_href: null
					}
				})
			]
		});
		const index = buildRequestActivityIndex(dataset);
		expect(activityIdForRequest(index, 201)).toBe('requests:201:started');
	});

	it('ignores an activity with no request_id', () => {
		const dataset = normalizeFlowDataset({
			sessionId: 'sid',
			actors: [flowActor('worker-a')],
			activities: [
				flowActivity('messages:m1:queued', {
					kind: 'messages',
					source: {
						table: 'session_messages',
						row_id: 'm1',
						session_id: 'sid',
						request_id: null,
						detail_href: null
					}
				})
			]
		});
		const index = buildRequestActivityIndex(dataset);
		expect(index.size).toBe(0);
	});
});
