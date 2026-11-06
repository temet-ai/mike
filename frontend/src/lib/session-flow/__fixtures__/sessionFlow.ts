import type { SessionFlowActivity } from '$lib/api/generated/SessionFlowActivity';
import type { SessionFlowActor } from '$lib/api/generated/SessionFlowActor';
import type { SessionFlowCoverage } from '$lib/api/generated/SessionFlowCoverage';
import { normalizeFlowDataset } from '../projection';
import type { FlowDataset } from '../types';

export const FLOW_SESSION_ID = '00000000-0000-4000-8000-000000000001';
export const FLOW_START_MS = Date.parse('2026-09-10T12:00:00Z');
export const flowTime = (offset: number): string => new Date(FLOW_START_MS + offset).toISOString();
export function flowActor(
	id = 'worker-a',
	overrides: Partial<SessionFlowActor> = {}
): SessionFlowActor {
	return {
		id,
		kind: 'worker',
		session_id: FLOW_SESSION_ID,
		label: 'Worker',
		producer_id: id,
		provenance: 'producer_header',
		parent_actor_id: null,
		source: null,
		...overrides
	};
}
export function flowActivity(
	id = 'requests:9007199254740993:started',
	overrides: Partial<SessionFlowActivity> = {}
): SessionFlowActivity {
	return {
		id,
		phase: 'started',
		actor_id: 'worker-a',
		source: {
			table: 'requests',
			row_id: '9007199254740993',
			session_id: FLOW_SESSION_ID,
			request_id: '9007199254740993',
			detail_href: '/api/requests/9007199254740993'
		},
		kind: 'requests',
		occurred_at: flowTime(0),
		recorded_at: flowTime(0),
		ended_at: null,
		time_semantics: 'capture_interval',
		lifecycle: 'started',
		outcome: null,
		source_revision: null,
		metadata: {
			tool_name: null,
			wire_tool_use_id: null,
			status_code: null,
			model: null,
			latency_ms: null,
			input_tokens: null,
			output_tokens: null,
			stage: null,
			role: null,
			steerer_id: null,
			turn_id: null,
			cost_usd: null,
			scope: null,
			verdict: null,
			has_verdict: null,
			attempt: null,
			body_available: null,
			heuristic_instance_id: null,
			agent_name: null,
			dispatch_description: null,
			firing_rule_count: null
		},
		...overrides
	};
}
export function flowCoverage(overrides: Partial<SessionFlowCoverage> = {}): SessionFlowCoverage {
	return {
		component: 'requests',
		lower_id: null,
		upper_id: null,
		returned_through_id: null,
		exhausted: true,
		retention: 'unknown',
		capture_complete: null,
		latest_only: false,
		best_effort: false,
		omitted_components: [],
		server_observed_at: flowTime(4000),
		stale_reason: null,
		...overrides
	};
}
export function flowDataset(
	activities: SessionFlowActivity[] = [
		flowActivity('a', { ended_at: flowTime(4000) }),
		flowActivity('b')
	]
): FlowDataset {
	return normalizeFlowDataset({
		sessionId: FLOW_SESSION_ID,
		actors: [flowActor(), flowActor('worker-b')],
		activities,
		coverage: [flowCoverage()]
	});
}
export function emptyFlowDataset(): FlowDataset {
	return flowDataset([]);
}
