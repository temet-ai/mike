import type { SessionFlowActivity } from '$lib/api/generated/SessionFlowActivity';
import type { SessionFlowActor } from '$lib/api/generated/SessionFlowActor';
import type { SessionFlowRelationship } from '$lib/api/generated/SessionFlowRelationship';
import type { SessionFlowRelationshipKind } from '$lib/api/generated/SessionFlowRelationshipKind';
import type { SessionFlowSource } from '$lib/api/generated/SessionFlowSource';
import { normalizeFlowDataset } from '../projection';
import type { FlowDataset } from '../types';
import { FLOW_SESSION_ID, flowActivity, flowActor, flowCoverage, flowTime } from './sessionFlow';

export const FLEET_SESSION_ID = FLOW_SESSION_ID;

let rowCounter = 0;
function nextRowId(): string {
	rowCounter += 1;
	return `fleet-row-${rowCounter}`;
}
function source(table: string, requestId: string | null = null): SessionFlowSource {
	const rowId = nextRowId();
	return {
		table,
		row_id: rowId,
		session_id: FLEET_SESSION_ID,
		request_id: requestId,
		detail_href: `/api/${table}/${rowId}`
	};
}

let relationshipCounter = 0;
function relationship(
	kind: SessionFlowRelationshipKind,
	fromId: string | null,
	toId: string | null,
	occurredAt: string
): SessionFlowRelationship {
	relationshipCounter += 1;
	return {
		id: `fleet-rel-${relationshipCounter}`,
		from_id: fromId,
		to_id: toId,
		kind,
		source: source('relationships'),
		phase: kind,
		occurred_at: occurredAt,
		recorded_at: occurredAt
	};
}

const actors: SessionFlowActor[] = [];
const activities: SessionFlowActivity[] = [];
const relationships: SessionFlowRelationship[] = [];

export const FLEET_COORDINATOR_ID = `session:${FLEET_SESSION_ID}`;
actors.push(
	flowActor(FLEET_COORDINATOR_ID, {
		kind: 'session',
		label: 'Coordinator',
		session_id: FLEET_SESSION_ID
	})
);
for (const [n, startOffset, endOffset] of [
	[1, 0, 20_000],
	[2, 25_000, 45_000],
	[3, 130_000, null]
] as const) {
	activities.push(
		flowActivity(`coordinator-request-${n}`, {
			actor_id: FLEET_COORDINATOR_ID,
			source: source('requests'),
			occurred_at: flowTime(startOffset),
			ended_at: endOffset === null ? null : flowTime(endOffset),
			lifecycle: endOffset === null ? 'started' : 'completed'
		})
	);
}

export const FLEET_UPSTREAM_ID = `upstream:${FLEET_SESSION_ID}`;
actors.push(
	flowActor(FLEET_UPSTREAM_ID, { kind: 'upstream', label: 'Anthropic', session_id: null })
);

export const peakMs = Date.parse(flowTime(123_000));

interface DispatchAgentPlan {
	n: number;
	dispatchObservedOffset: number;
	requests: readonly { startOffset: number; endOffset: number | null; error?: boolean }[];
	pending?: boolean;
	headerMerge?: boolean;
}

const AGENT_PLANS: readonly DispatchAgentPlan[] = [
	{
		n: 1,
		dispatchObservedOffset: 0,
		requests: [{ startOffset: 1_000, endOffset: null }],
		headerMerge: true
	},
	{ n: 2, dispatchObservedOffset: 0, requests: [{ startOffset: 1_000, endOffset: null }] },
	{ n: 3, dispatchObservedOffset: 0, requests: [{ startOffset: 1_000, endOffset: null }] },
	{ n: 4, dispatchObservedOffset: 0, requests: [{ startOffset: 1_000, endOffset: null }] },
	{ n: 5, dispatchObservedOffset: 0, requests: [{ startOffset: 1_000, endOffset: null }] },
	{
		n: 6,
		dispatchObservedOffset: 30_000,
		requests: [{ startOffset: 31_000, endOffset: 31_500, error: true }]
	},
	{ n: 7, dispatchObservedOffset: 30_000, requests: [{ startOffset: 31_000, endOffset: null }] },
	{ n: 8, dispatchObservedOffset: 30_000, requests: [{ startOffset: 31_000, endOffset: null }] },
	{ n: 9, dispatchObservedOffset: 30_000, requests: [{ startOffset: 31_000, endOffset: null }] },
	{ n: 10, dispatchObservedOffset: 30_000, requests: [{ startOffset: 31_000, endOffset: null }] },
	{ n: 11, dispatchObservedOffset: 60_000, requests: [{ startOffset: 61_000, endOffset: null }] },
	{ n: 12, dispatchObservedOffset: 60_000, requests: [{ startOffset: 61_000, endOffset: null }] },
	{ n: 13, dispatchObservedOffset: 60_000, requests: [{ startOffset: 61_000, endOffset: null }] },
	{ n: 14, dispatchObservedOffset: 60_000, requests: [{ startOffset: 61_000, endOffset: null }] },
	{ n: 15, dispatchObservedOffset: 60_000, requests: [{ startOffset: 61_000, endOffset: null }] },
	{ n: 16, dispatchObservedOffset: 90_000, requests: [{ startOffset: 91_000, endOffset: null }] },
	{ n: 17, dispatchObservedOffset: 90_000, requests: [{ startOffset: 91_000, endOffset: null }] },
	{ n: 18, dispatchObservedOffset: 90_000, requests: [{ startOffset: 91_000, endOffset: null }] },
	{ n: 19, dispatchObservedOffset: 90_000, requests: [{ startOffset: 91_000, endOffset: null }] },
	{ n: 20, dispatchObservedOffset: 90_000, requests: [{ startOffset: 91_000, endOffset: null }] },
	{ n: 21, dispatchObservedOffset: 120_000, requests: [{ startOffset: 121_000, endOffset: null }] },
	{ n: 22, dispatchObservedOffset: 120_000, requests: [{ startOffset: 123_000, endOffset: null }] },
	{ n: 23, dispatchObservedOffset: 120_000, requests: [], pending: true },
	{ n: 24, dispatchObservedOffset: 120_000, requests: [], pending: true }
];

export const FLEET_PENDING_DISPATCH_IDS: string[] = [];
export let FLEET_A1_DISPATCH_ID = '';
export let FLEET_A1_INSTANCE_WORKER_ID = '';
export let FLEET_A1_HEADER_WORKER_ID = '';
export let FLEET_A6_ERROR_REQUEST_ID = '';
export let FLEET_A2_ACTOR_ID = '';

for (const plan of AGENT_PLANS) {
	const dispatchId = `dispatch:${FLEET_SESSION_ID}:${plan.n}`;
	const dispatchSource = source('agents.dispatches');
	const dispatchActivity = flowActivity(`${dispatchId}:observed`, {
		actor_id: dispatchId,
		source: dispatchSource,
		kind: 'dispatches',
		phase: 'observed',
		lifecycle: 'observed',
		time_semantics: 'recorded_observation',
		occurred_at: flowTime(plan.dispatchObservedOffset),
		ended_at: null
	});
	if (plan.headerMerge) dispatchActivity.metadata.heuristic_instance_id = `heuristic-${plan.n}`;
	actors.push(
		flowActor(dispatchId, {
			kind: 'unresolved_dispatch',
			label: `Dispatch ${plan.n}`,
			provenance: 'heuristic'
		})
	);
	activities.push(dispatchActivity);

	if (plan.pending) {
		FLEET_PENDING_DISPATCH_IDS.push(dispatchId);
		continue;
	}

	const instanceWorkerId = `worker:${FLEET_SESSION_ID}:instance:${plan.n}`;
	actors.push(
		flowActor(instanceWorkerId, {
			kind: 'worker',
			label: `Agent ${plan.n}`,
			provenance: 'explicit_correlation',
			parent_actor_id: dispatchId,
			producer_id: instanceWorkerId
		})
	);
	relationships.push(
		relationship(
			'explicit_parent',
			dispatchActivity.id,
			instanceWorkerId,
			flowTime(plan.dispatchObservedOffset)
		)
	);

	let requestActorId = instanceWorkerId;
	if (plan.headerMerge) {
		const headerWorkerId = `worker:${FLEET_SESSION_ID}:agent-${plan.n}`;
		actors.push(
			flowActor(headerWorkerId, {
				kind: 'worker',
				label: `Agent ${plan.n} (header)`,
				provenance: 'producer_header',
				producer_id: `heuristic-${plan.n}`
			})
		);
		requestActorId = headerWorkerId;
		FLEET_A1_HEADER_WORKER_ID = headerWorkerId;
	}

	for (const [i, request] of plan.requests.entries()) {
		const requestId = `agent-${plan.n}-request-${i}`;
		activities.push(
			flowActivity(requestId, {
				actor_id: requestActorId,
				source: source('requests'),
				occurred_at: flowTime(request.startOffset),
				ended_at: request.endOffset === null ? null : flowTime(request.endOffset),
				lifecycle: request.endOffset === null ? 'started' : 'completed',
				outcome: request.error ? 'error' : null,
				metadata: {
					...flowActivity().metadata,
					status_code: request.error ? 500 : null
				}
			})
		);
		if (plan.n === 6 && request.error) FLEET_A6_ERROR_REQUEST_ID = requestId;
	}

	if (plan.n === 1) {
		FLEET_A1_DISPATCH_ID = dispatchId;
		FLEET_A1_INSTANCE_WORKER_ID = instanceWorkerId;
	}
	if (plan.n === 2) FLEET_A2_ACTOR_ID = instanceWorkerId;
}

export const FLEET_PEER_ID = `peer:${FLEET_SESSION_ID}`;
export const FLEET_MAILBOX_ID = `mailbox:${FLEET_SESSION_ID}`;
actors.push(flowActor(FLEET_PEER_ID, { kind: 'peer', label: 'Peer' }));
actors.push(flowActor(FLEET_MAILBOX_ID, { kind: 'mike', label: 'Mailbox' }));

export let FLEET_FIRST_PEER_MESSAGE_SENT_MS = 0;
for (let n = 1; n <= 14; n += 1) {
	const queuedOffset = 5_000 + n * 3_000;
	const messageSource = source('mailbox.messages');
	activities.push(
		flowActivity(`message-${n}-queued`, {
			actor_id: FLEET_MAILBOX_ID,
			source: messageSource,
			kind: 'messages',
			phase: 'queued',
			lifecycle: 'queued',
			time_semantics: 'recorded_observation',
			occurred_at: flowTime(queuedOffset),
			ended_at: null
		})
	);
	if (n <= 4) continue;
	const sentOffset = queuedOffset + 500;
	activities.push(
		flowActivity(`message-${n}-sent`, {
			actor_id: FLEET_PEER_ID,
			source: { ...messageSource, row_id: messageSource.row_id },
			kind: 'messages',
			phase: 'sent',
			lifecycle: 'delivered',
			time_semantics: 'recorded_observation',
			occurred_at: flowTime(sentOffset),
			ended_at: null
		})
	);
	if (n === 5) FLEET_FIRST_PEER_MESSAGE_SENT_MS = Date.parse(flowTime(sentOffset));
	if (n <= 9) continue;
	const consumedOffset = sentOffset + 500;
	activities.push(
		flowActivity(`message-${n}-consumed`, {
			actor_id: FLEET_MAILBOX_ID,
			source: { ...messageSource, row_id: messageSource.row_id },
			kind: 'messages',
			phase: 'consumed',
			lifecycle: 'consumed',
			time_semantics: 'recorded_observation',
			occurred_at: flowTime(consumedOffset),
			ended_at: null
		})
	);
}

export const FLEET_SIDECAR_IDS = ['sidecar-alpha', 'sidecar-beta', 'sidecar-gamma'].map(
	(name) => `sidecar:${FLEET_SESSION_ID}:${name}`
);
export let FLEET_LATE_SIDECAR_REQUEST_ID = '';
const SIDECAR_COUNTS = [21, 21, 20];
for (const [classIndex, sidecarId] of FLEET_SIDECAR_IDS.entries()) {
	actors.push(flowActor(sidecarId, { kind: 'sidecar', label: `Sidecar ${classIndex + 1}` }));
	const count = SIDECAR_COUNTS[classIndex]!;
	for (let i = 0; i < count; i += 1) {
		const baseOffset = 2_000 + classIndex * 500 + i * 1_500;
		const late = classIndex === 0 && i === 0;
		const requestId = `sidecar-${classIndex}-request-${i}`;
		activities.push(
			flowActivity(requestId, {
				actor_id: sidecarId,
				source: source('requests'),
				occurred_at: flowTime(baseOffset),
				ended_at: flowTime(baseOffset + (late ? 90_000 : 400)),
				lifecycle: 'completed'
			})
		);
		if (late) FLEET_LATE_SIDECAR_REQUEST_ID = requestId;
	}
}

export const FLEET_TOOL_ACTOR_IDS = {
	shell: ['tool:bash-1', 'tool:bash-2', 'tool:bash-3'],
	files: ['tool:read-1', 'tool:edit-1'],
	browser: ['tool:browser-1'],
	mikeMcp: 'tool:mcp-mike-get-session',
	githubMcp: 'tool:mcp-github-search-code'
} as const;
const TOOL_LABELS: Readonly<Record<string, string>> = {
	'tool:bash-1': 'Bash',
	'tool:bash-2': 'Bash',
	'tool:bash-3': 'Bash',
	'tool:read-1': 'Read',
	'tool:edit-1': 'Edit',
	'tool:browser-1': 'Browser',
	'tool:mcp-mike-get-session': 'mcp__mike__get_session',
	'tool:mcp-github-search-code': 'mcp__github__search_code'
};
export const FLEET_CALLER_TOOL_ACTOR_ID = FLEET_TOOL_ACTOR_IDS.shell[0];
export let FLEET_A1_REQUEST_ACTIVITY_ID = '';

const allToolActorIds = [
	...FLEET_TOOL_ACTOR_IDS.shell,
	...FLEET_TOOL_ACTOR_IDS.files,
	...FLEET_TOOL_ACTOR_IDS.browser,
	FLEET_TOOL_ACTOR_IDS.mikeMcp,
	FLEET_TOOL_ACTOR_IDS.githubMcp
];
for (const toolActorId of allToolActorIds) {
	actors.push(flowActor(toolActorId, { kind: 'tool_service', label: TOOL_LABELS[toolActorId]! }));
	const wireToolUseId = `wire-${toolActorId}`;
	activities.push(
		flowActivity(`${toolActorId}-call`, {
			actor_id: toolActorId,
			source: source('tools.uses'),
			kind: 'tools',
			phase: 'observed',
			lifecycle: 'observed',
			time_semantics: 'recorded_observation',
			occurred_at: flowTime(120_000),
			ended_at: null,
			metadata: {
				...flowActivity().metadata,
				tool_name: TOOL_LABELS[toolActorId]!,
				wire_tool_use_id: wireToolUseId
			}
		})
	);
}
FLEET_A1_REQUEST_ACTIVITY_ID = 'agent-1-request-0';
relationships.push(
	relationship(
		'request_tool',
		FLEET_A1_REQUEST_ACTIVITY_ID,
		`${FLEET_CALLER_TOOL_ACTOR_ID}-call`,
		flowTime(1_500)
	)
);
export const FLEET_TOOL_CALLERS: ReadonlyArray<
	readonly [toolActorId: string, requestActivityId: string]
> = [
	['tool:bash-2', 'agent-4-request-0'],
	['tool:bash-3', 'agent-9-request-0'],
	['tool:read-1', 'agent-15-request-0'],
	['tool:edit-1', 'agent-17-request-0'],
	['tool:browser-1', 'agent-21-request-0'],
	['tool:mcp-mike-get-session', 'agent-11-request-0'],
	['tool:mcp-github-search-code', 'agent-20-request-0']
];
for (const [toolActorId, requestId] of FLEET_TOOL_CALLERS) {
	if (!activities.some((a) => a.id === requestId))
		throw new Error(`fleet fixture: no request ${requestId} for ${toolActorId}`);
	relationships.push(
		relationship('request_tool', requestId, `${toolActorId}-call`, flowTime(120_500))
	);
}

const MIKE_ROLE_LABELS = [
	'Manifest Stages',
	'Lesson Injections',
	'Verifier Evaluations',
	'Judgments',
	'Llm Calls',
	'Embeddings',
	'Consolidation',
	'Interventions',
	'Analyzers'
] as const;
const MIKE_ROLE_IDS = Object.fromEntries(
	MIKE_ROLE_LABELS.map((label) => [
		label,
		`mike:${FLEET_SESSION_ID}:${label.toLowerCase().replace(/ /g, '_')}`
	])
) as Record<(typeof MIKE_ROLE_LABELS)[number], string>;
for (const label of MIKE_ROLE_LABELS)
	actors.push(flowActor(MIKE_ROLE_IDS[label], { kind: 'mike', label }));

export let FLEET_FIRST_LESSON_INJECTION_MS = 0;

activities.push(
	flowActivity('manifest-stage-filter', {
		actor_id: MIKE_ROLE_IDS['Manifest Stages'],
		source: source('mike.manifest_stages'),
		kind: 'manifest_stages',
		phase: 'observed',
		lifecycle: 'observed',
		time_semantics: 'recorded_observation',
		occurred_at: flowTime(40_000),
		ended_at: null,
		metadata: { ...flowActivity().metadata, stage: 'filter' }
	})
);
activities.push(
	flowActivity('manifest-stage-transform', {
		actor_id: MIKE_ROLE_IDS['Manifest Stages'],
		source: source('mike.manifest_stages'),
		kind: 'manifest_stages',
		phase: 'observed',
		lifecycle: 'observed',
		time_semantics: 'recorded_observation',
		occurred_at: flowTime(40_500),
		ended_at: null,
		metadata: { ...flowActivity().metadata, stage: 'reorder' }
	})
);

activities.push(
	flowActivity('lesson-injection-1', {
		actor_id: MIKE_ROLE_IDS['Lesson Injections'],
		source: source('mike.lesson_injections'),
		kind: 'lesson_injections',
		phase: 'observed',
		lifecycle: 'observed',
		time_semantics: 'recorded_observation',
		occurred_at: flowTime(15_000),
		ended_at: null
	})
);
FLEET_FIRST_LESSON_INJECTION_MS = Date.parse(flowTime(15_000));

const verifierActivity = flowActivity('verifier-evaluation-1', {
	actor_id: MIKE_ROLE_IDS['Verifier Evaluations'],
	source: source('mike.verifier_evaluations'),
	kind: 'verifier_evaluations',
	phase: 'observed',
	lifecycle: 'observed',
	time_semantics: 'recorded_observation',
	occurred_at: flowTime(62_000),
	ended_at: null
});
activities.push(verifierActivity);
relationships.push(
	relationship('activity_request', verifierActivity.id, 'agent-2-request-0', flowTime(62_000))
);

activities.push(
	flowActivity('judgment-near-peak', {
		actor_id: MIKE_ROLE_IDS.Judgments,
		source: source('mike.judgments'),
		kind: 'judgments',
		phase: 'observed',
		lifecycle: 'observed',
		time_semantics: 'recorded_observation',
		occurred_at: flowTime(118_000),
		ended_at: null
	})
);

activities.push(
	flowActivity('llm-call-judge', {
		actor_id: MIKE_ROLE_IDS['Llm Calls'],
		source: source('mike.llm_calls'),
		kind: 'llm_calls',
		phase: 'observed',
		lifecycle: 'observed',
		time_semantics: 'recorded_observation',
		occurred_at: flowTime(41_000),
		ended_at: null,
		metadata: { ...flowActivity().metadata, role: 'judge' }
	})
);
activities.push(
	flowActivity('llm-call-verifier', {
		actor_id: MIKE_ROLE_IDS['Llm Calls'],
		source: source('mike.llm_calls'),
		kind: 'llm_calls',
		phase: 'observed',
		lifecycle: 'observed',
		time_semantics: 'recorded_observation',
		occurred_at: flowTime(62_500),
		ended_at: null,
		metadata: { ...flowActivity().metadata, role: 'verifier' }
	})
);

activities.push(
	flowActivity('embedding-1', {
		actor_id: MIKE_ROLE_IDS.Embeddings,
		source: source('mike.embeddings'),
		kind: 'embeddings',
		phase: 'observed',
		lifecycle: 'observed',
		time_semantics: 'recorded_observation',
		occurred_at: flowTime(70_000),
		ended_at: null
	})
);

activities.push(
	flowActivity('intervention-1', {
		actor_id: MIKE_ROLE_IDS.Interventions,
		source: source('mike.interventions'),
		kind: 'interventions',
		phase: 'observed',
		lifecycle: 'observed',
		time_semantics: 'recorded_observation',
		occurred_at: flowTime(75_000),
		ended_at: null
	})
);

export const FLEET_ACTORS: readonly SessionFlowActor[] = actors;
export const FLEET_ACTIVITIES: readonly SessionFlowActivity[] = activities;
export const FLEET_RELATIONSHIPS: readonly SessionFlowRelationship[] = relationships;

export function fleetDataset(options: { coordinatorTurns?: number } = {}): FlowDataset {
	const coordinatorTurns = options.coordinatorTurns ?? 3;
	const extraCoordinatorActivities: SessionFlowActivity[] = [];
	for (let n = 4; n <= coordinatorTurns; n += 1) {
		const startOffset = 130_000 + 10_000 * (n - 3);
		const isLast = n === coordinatorTurns;
		const endOffset = isLast ? null : startOffset + 8_000;
		extraCoordinatorActivities.push(
			flowActivity(`coordinator-request-${n}`, {
				actor_id: FLEET_COORDINATOR_ID,
				source: source('requests'),
				occurred_at: flowTime(startOffset),
				ended_at: endOffset === null ? null : flowTime(endOffset),
				lifecycle: endOffset === null ? 'started' : 'completed'
			})
		);
	}
	return normalizeFlowDataset({
		sessionId: FLEET_SESSION_ID,
		actors,
		activities: extraCoordinatorActivities.length
			? [...activities, ...extraCoordinatorActivities]
			: activities,
		relationships,
		coverage: [flowCoverage()]
	});
}
