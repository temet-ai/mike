import { bornAtMs } from './born';
import { buildAgentIdentities, toolFamilyOf } from './grouping';
import { type FlowRole, identityOf, rolesOf } from './roles';
import { isMessageDelivery } from './messagePhases';
import { chipFor, EVALUATIVE_CHIPS } from './serviceRoles';
import type { FlowActivity, FlowDataset } from './types';
import type { IdentityFamily, TraceKind } from '$lib/board/types';

export const RECENT_EVENT_WINDOW_MS = 8_000;
export const DISPATCH_TRACE_WINDOW_MS = 25_000;

export interface FlowTraceEntry {
	id: string;
	kind: TraceKind;
	from: string;
	to: string;
	identity: IdentityFamily;
	reverse: boolean;
	phase: number;
	opacity: number;
}

function packetPhase(atMs: number, originMs: number, periodMs = 1_200): number {
	const elapsed = atMs - originMs;
	if (elapsed <= 0) return 0;
	return (elapsed % periodMs) / periodMs;
}

function captureAndResponseTraces(
	dataset: FlowDataset,
	atMs: number,
	roles: ReadonlyMap<string, FlowRole>
): FlowTraceEntry[] {
	const entries: FlowTraceEntry[] = [];
	for (const activity of dataset.activities.values()) {
		if (
			activity.kind !== 'requests' ||
			activity.time_semantics !== 'capture_interval' ||
			activity.atMs === null
		)
			continue;
		if (activity.atMs > atMs) continue;
		const role = roles.get(activity.actor_id);
		if (role === undefined) continue;
		const open = activity.endedAtMs === null || activity.endedAtMs > atMs;
		if (open) {
			entries.push({
				id: `capture:${activity.id}`,
				kind: 'capture',
				from: activity.actor_id,
				to: 'upstream',
				identity: identityOf(role),
				reverse: false,
				phase: packetPhase(atMs, activity.atMs),
				opacity: 1
			});
			continue;
		}
		const sinceClosed = atMs - activity.endedAtMs!;
		if (sinceClosed >= 0 && sinceClosed <= RECENT_EVENT_WINDOW_MS) {
			entries.push({
				id: `response:${activity.id}`,
				kind: 'response',
				from: 'upstream',
				to: activity.actor_id,
				identity: 'outside',
				reverse: true,
				phase: packetPhase(atMs, activity.endedAtMs!),
				opacity: 1 - sinceClosed / RECENT_EVENT_WINDOW_MS
			});
		}
	}
	return entries;
}

function toolFamilyRole(actor: { label: string }, role: FlowRole): string {
	if (role === 'mike-mcp') return 'mike-mcp';
	if (role.startsWith('other-mcp:')) return role.slice('other-mcp:'.length);
	return toolFamilyOf(actor.label);
}

const INTERNET_FAMILIES = new Set<string>(['browser']);

function isExternalMcp(role: FlowRole): boolean {
	return role.startsWith('other-mcp:');
}

function toolTraces(
	dataset: FlowDataset,
	atMs: number,
	roles: ReadonlyMap<string, FlowRole>
): FlowTraceEntry[] {
	const entries: FlowTraceEntry[] = [];
	const resultAtMsByWireId = new Map<string, number>();
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'results' || activity.atMs === null) continue;
		const wireId = activity.metadata.wire_tool_use_id;
		if (wireId === null) continue;
		const existing = resultAtMsByWireId.get(wireId);
		if (existing === undefined || activity.atMs < existing)
			resultAtMsByWireId.set(wireId, activity.atMs);
	}
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'tools' || activity.atMs === null || activity.atMs > atMs) continue;
		const actor = dataset.actors.get(activity.actor_id);
		if (!actor) continue;
		const role = roles.get(actor.id)!;
		const wireId = activity.metadata.wire_tool_use_id;
		const resultAtMs = wireId === null ? undefined : resultAtMsByWireId.get(wireId);
		const family = toolFamilyRole(actor, role);
		const internet = INTERNET_FAMILIES.has(family) || isExternalMcp(role);
		const open = resultAtMs === undefined || resultAtMs > atMs;
		if (open) {
			entries.push({
				id: `toolcall:${activity.id}`,
				kind: 'toolcall',
				from: activity.actor_id,
				to: activity.actor_id,
				identity: identityOf(role),
				reverse: false,
				phase: packetPhase(atMs, activity.atMs),
				opacity: 1
			});
			if (internet)
				entries.push({
					id: `internet:${activity.id}`,
					kind: 'internet',
					from: activity.actor_id,
					to: 'upstream',
					identity: identityOf(role),
					reverse: false,
					phase: packetPhase(atMs, activity.atMs),
					opacity: 1
				});
			continue;
		}
		const sinceResult = atMs - resultAtMs!;
		if (sinceResult >= 0 && sinceResult <= RECENT_EVENT_WINDOW_MS)
			entries.push({
				id: `return:${activity.id}`,
				kind: 'return',
				from: activity.actor_id,
				to: activity.actor_id,
				identity: identityOf(role),
				reverse: true,
				phase: packetPhase(atMs, resultAtMs!),
				opacity: 1 - sinceResult / RECENT_EVENT_WINDOW_MS
			});
	}
	return entries;
}

function targetActorId(
	dataset: FlowDataset,
	activity: FlowActivity,
	coordinatorId: string | null
): string | null {
	for (const relationship of dataset.relationships.values()) {
		if (relationship.kind !== 'activity_request' || relationship.from_id !== activity.id) continue;
		if (relationship.to_id === null) continue;
		const targetActivity = dataset.activities.get(relationship.to_id);
		if (targetActivity) return targetActivity.actor_id;
	}
	return coordinatorId;
}

function serviceFiringTraces(
	dataset: FlowDataset,
	atMs: number,
	roles: ReadonlyMap<string, FlowRole>,
	coordinatorId: string | null
): FlowTraceEntry[] {
	const entries: FlowTraceEntry[] = [];
	for (const activity of dataset.activities.values()) {
		if (activity.atMs === null || activity.atMs > atMs) continue;
		const actor = dataset.actors.get(activity.actor_id);
		if (!actor) continue;
		const chip = chipFor(actor, activity);
		if (chip === null) continue;
		const sinceFired = atMs - activity.atMs;
		if (sinceFired < 0 || sinceFired > RECENT_EVENT_WINDOW_MS) continue;
		const to = targetActorId(dataset, activity, coordinatorId);
		if (to === null) continue;
		const evaluative = EVALUATIVE_CHIPS.has(chip);
		entries.push({
			id: `${evaluative ? 'mikeout' : 'audit'}:${activity.id}`,
			kind: evaluative ? 'mikeout' : 'audit',
			from: activity.actor_id,
			to,
			identity: identityOf(roles.get(activity.actor_id) ?? 'service:unknown'),
			reverse: false,
			phase: packetPhase(atMs, activity.atMs),
			opacity: 1 - sinceFired / RECENT_EVENT_WINDOW_MS
		});
	}
	return entries;
}

function dispatchTraces(
	dataset: FlowDataset,
	atMs: number,
	coordinatorId: string | null,
	selectedActorId: string | null
): FlowTraceEntry[] {
	if (coordinatorId === null) return [];
	const entries: FlowTraceEntry[] = [];
	for (const identity of buildAgentIdentities(dataset)) {
		const representativeId = identity.dispatchActorId ?? identity.workerActorIds[0]!;
		const born = bornAtMs(dataset, representativeId);
		if (born === null || born > atMs) continue;
		const withinBirthWindow = atMs - born <= DISPATCH_TRACE_WINDOW_MS;
		const selected = selectedActorId !== null && identity.actorIds.includes(selectedActorId);
		if (!withinBirthWindow && !selected) continue;
		entries.push({
			id: `dispatch:${identity.key}`,
			kind: 'dispatch',
			from: coordinatorId,
			to: identity.workerActorIds[0] ?? representativeId,
			identity: identityOf('coordinator'),
			reverse: false,
			phase: 0,
			opacity: selected ? 1 : 1 - (atMs - born) / DISPATCH_TRACE_WINDOW_MS
		});
	}
	return entries;
}

function messageTraces(
	dataset: FlowDataset,
	atMs: number,
	roles: ReadonlyMap<string, FlowRole>,
	coordinatorId: string | null
): FlowTraceEntry[] {
	const entries: FlowTraceEntry[] = [];
	if (coordinatorId !== null)
		entries.push({
			id: 'message:terminal-ask',
			kind: 'message',
			from: 'terminal',
			to: coordinatorId,
			identity: 'terminal',
			reverse: false,
			phase: packetPhase(atMs, 0),
			opacity: 1
		});
	for (const activity of dataset.activities.values()) {
		if (!isMessageDelivery(activity) || activity.atMs === null) continue;
		if (activity.atMs > atMs) continue;
		const sinceSent = atMs - activity.atMs;
		if (sinceSent > RECENT_EVENT_WINDOW_MS) continue;
		const role = roles.get(activity.actor_id);
		if (role === undefined) continue;
		entries.push({
			id: `message:${activity.id}`,
			kind: 'message',
			from: activity.actor_id,
			to: 'service:mailbox',
			identity: identityOf(role),
			reverse: false,
			phase: packetPhase(atMs, activity.atMs),
			opacity: 1 - sinceSent / RECENT_EVENT_WINDOW_MS
		});
	}
	return entries;
}

export function buildTraceModel(
	dataset: FlowDataset,
	atMs: number,
	selectedActorId: string | null = null
): readonly FlowTraceEntry[] {
	const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
	let coordinatorId: string | null = null;
	for (const actor of dataset.actors.values())
		if (roles.get(actor.id) === 'coordinator') coordinatorId = actor.id;
	return [
		...captureAndResponseTraces(dataset, atMs, roles),
		...toolTraces(dataset, atMs, roles),
		...serviceFiringTraces(dataset, atMs, roles, coordinatorId),
		...dispatchTraces(dataset, atMs, coordinatorId, selectedActorId),
		...messageTraces(dataset, atMs, roles, coordinatorId)
	];
}
