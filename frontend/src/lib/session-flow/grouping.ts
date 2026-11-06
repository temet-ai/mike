import type { SessionFlowActor } from '$lib/api/generated/SessionFlowActor';
import { activitiesOfActor } from './actorActivities';
import { compareBornAtMs } from './born';
import { compareFlowIds } from './intervalIndex';
import { type FlowRole, rolesOf } from './roles';
import type { FlowActivity, FlowDataset } from './types';

export type ToolFamily = 'shell' | 'files' | 'browser' | 'other';

const SHELL_TOOL_PATTERN = /bash|shell|exec|terminal|command/i;
const BROWSER_TOOL_PATTERN = /browser|navigate|screenshot|chrome|web(?!hook)/i;
const FILE_TOOL_PATTERN = /read|write|edit|glob|grep|\bls\b|file|notebook/i;

export function toolFamilyOf(name: string): ToolFamily {
	if (SHELL_TOOL_PATTERN.test(name)) return 'shell';
	if (BROWSER_TOOL_PATTERN.test(name)) return 'browser';
	if (FILE_TOOL_PATTERN.test(name)) return 'files';
	return 'other';
}

const TOOL_FAMILY_LABEL: Readonly<Record<ToolFamily, string>> = {
	shell: 'Shell',
	files: 'Files',
	browser: 'Browser',
	other: 'Other'
};

export interface FlowGroup {
	key: string;
	role: FlowRole;
	label: string;
	memberIds: readonly string[];
	counts: ReadonlyMap<string, number>;
}

function activityCountsByActor(dataset: FlowDataset): ReadonlyMap<string, number> {
	const counts = new Map<string, number>();
	for (const activity of dataset.activities.values())
		counts.set(activity.actor_id, (counts.get(activity.actor_id) ?? 0) + 1);
	return counts;
}

export function buildGroups(dataset: FlowDataset): readonly FlowGroup[] {
	const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
	const counts = activityCountsByActor(dataset);
	const buckets = new Map<string, { role: FlowRole; label: string; memberIds: string[] }>();
	for (const actor of dataset.actors.values()) {
		const role = roles.get(actor.id)!;
		let key: string | null = null;
		let label = '';
		if (role === 'sidecars') {
			key = 'sidecars';
			label = 'Sidecars';
		} else if (role === 'local-tools') {
			const family = toolFamilyOf(actor.label);
			key = `tools:${family}`;
			label = TOOL_FAMILY_LABEL[family];
		} else if (role === 'mike-mcp') {
			key = 'mike-mcp';
			label = 'Mike MCP';
		} else if (role.startsWith('other-mcp:')) {
			key = role;
			label = role.slice('other-mcp:'.length);
		}
		if (key === null) continue;
		const bucket = buckets.get(key) ?? { role, label, memberIds: [] };
		bucket.memberIds.push(actor.id);
		buckets.set(key, bucket);
	}
	return [...buckets.entries()]
		.sort(([a], [b]) => compareFlowIds(a, b))
		.map(([key, bucket]) => {
			const memberIds = bucket.memberIds.sort(compareFlowIds);
			return {
				key,
				role: bucket.role,
				label: bucket.label,
				memberIds,
				counts: new Map(memberIds.map((id) => [id, counts.get(id) ?? 0]))
			};
		});
}

export interface FlowAgentIdentity {
	key: string;
	actorIds: readonly string[];
	dispatchActorId: string | null;
	workerActorIds: readonly string[];
	pending: boolean;
	label: string;
	agentName: string | null;
	description: string | null;
}

function dispatchHeuristicIds(dataset: FlowDataset): ReadonlyMap<string, string> {
	const ids = new Map<string, string>();
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'dispatches') continue;
		const heuristicId = activity.metadata.heuristic_instance_id;
		if (heuristicId !== null) ids.set(activity.actor_id, heuristicId);
	}
	return ids;
}

function workersByProducerId(
	workers: readonly SessionFlowActor[]
): ReadonlyMap<string, SessionFlowActor[]> {
	const byProducerId = new Map<string, SessionFlowActor[]>();
	for (const worker of workers) {
		if (worker.producer_id === null) continue;
		const list = byProducerId.get(worker.producer_id);
		if (list) list.push(worker);
		else byProducerId.set(worker.producer_id, [worker]);
	}
	return byProducerId;
}

export function dispatchActivityOf(
	dataset: FlowDataset,
	dispatchActorId: string
): FlowActivity | undefined {
	for (const activity of activitiesOfActor(dataset, dispatchActorId))
		if (activity.kind === 'dispatches') return activity;
	return undefined;
}

function disambiguateLabels(identities: FlowAgentIdentity[]): FlowAgentIdentity[] {
	const total = new Map<string, number>();
	for (const identity of identities)
		total.set(identity.label, (total.get(identity.label) ?? 0) + 1);
	const seen = new Map<string, number>();
	return identities.map((identity) => {
		if ((total.get(identity.label) ?? 0) < 2 || identity.label === 'PENDING IDENTITY')
			return identity;
		const n = (seen.get(identity.label) ?? 0) + 1;
		seen.set(identity.label, n);
		return { ...identity, label: `${identity.label} · ${n}` };
	});
}

export function buildAgentIdentities(dataset: FlowDataset): readonly FlowAgentIdentity[] {
	const dispatches = [...dataset.actors.values()].filter(
		(actor) => actor.kind === 'unresolved_dispatch'
	);
	const workers = [...dataset.actors.values()].filter((actor) => actor.kind === 'worker');
	const heuristicIds = dispatchHeuristicIds(dataset);
	const byProducerId = workersByProducerId(workers);
	const mergedWorkerIds = new Set<string>();
	const identities: FlowAgentIdentity[] = [];

	for (const dispatch of dispatches) {
		const linked: SessionFlowActor[] = [];
		for (const worker of workers) if (worker.parent_actor_id === dispatch.id) linked.push(worker);
		const linkedProducerIds = new Set(
			linked.map((worker) => worker.producer_id).filter((id): id is string => id !== null)
		);
		for (const worker of workers) {
			if (linked.includes(worker)) continue;
			if (worker.producer_id !== null && linkedProducerIds.has(worker.producer_id))
				linked.push(worker);
		}
		const heuristicId = heuristicIds.get(dispatch.id);
		if (heuristicId !== undefined)
			for (const worker of byProducerId.get(heuristicId) ?? [])
				if (!linked.includes(worker)) linked.push(worker);
		for (const worker of linked) mergedWorkerIds.add(worker.id);
		const workerActorIds = linked.map((worker) => worker.id).sort(compareFlowIds);
		const meta = dispatchActivityOf(dataset, dispatch.id)?.metadata;
		const agentName = meta?.agent_name ?? null;
		const description = meta?.dispatch_description ?? null;
		identities.push({
			key: dispatch.id,
			actorIds: [dispatch.id, ...workerActorIds].sort(compareFlowIds),
			dispatchActorId: dispatch.id,
			workerActorIds,
			pending: linked.length === 0,
			label: agentName ?? (linked.length ? linked[0]!.label : 'PENDING IDENTITY'),
			agentName,
			description
		});
	}
	for (const worker of workers) {
		if (mergedWorkerIds.has(worker.id)) continue;
		identities.push({
			key: worker.id,
			actorIds: [worker.id],
			dispatchActorId: null,
			workerActorIds: [worker.id],
			pending: false,
			label: worker.label,
			agentName: null,
			description: null
		});
	}

	const compare = compareBornAtMs(dataset);
	const ordered = identities
		.slice()
		.sort((a, b) =>
			compare(a.dispatchActorId ?? a.workerActorIds[0]!, b.dispatchActorId ?? b.workerActorIds[0]!)
		);
	return disambiguateLabels(ordered);
}
