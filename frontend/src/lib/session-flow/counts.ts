import { toolFamilyOf } from './grouping';
import { sceneAt } from './projection';
import { type FlowRole, rolesOf } from './roles';
import type { FlowDataset } from './types';

export interface FlowCounts {
	atMs: number;
	openCapturesByRole: ReadonlyMap<FlowRole, number>;
	distinctAgentsWithOpenCaptures: number;
	openToolCallsByFamily: ReadonlyMap<string, number>;
	errors: number;
	lessonInjectionsSoFar: number;
	messagesQueued: number;
}

function openToolCallsByFamily(
	dataset: FlowDataset,
	atMs: number,
	roles: ReadonlyMap<string, FlowRole>
): ReadonlyMap<string, number> {
	const resultedWireIds = new Set<string>();
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'results' || activity.atMs === null || activity.atMs > atMs) continue;
		const wireId = activity.metadata.wire_tool_use_id;
		if (wireId !== null) resultedWireIds.add(wireId);
	}
	const counts = new Map<string, number>();
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'tools' || activity.atMs === null || activity.atMs > atMs) continue;
		const wireId = activity.metadata.wire_tool_use_id;
		if (wireId !== null && resultedWireIds.has(wireId)) continue;
		const actor = dataset.actors.get(activity.actor_id);
		if (!actor) continue;
		const role = roles.get(actor.id);
		const family =
			role === 'mike-mcp'
				? 'mike-mcp'
				: role?.startsWith('other-mcp:')
					? role.slice('other-mcp:'.length)
					: toolFamilyOf(actor.label);
		counts.set(family, (counts.get(family) ?? 0) + 1);
	}
	return counts;
}

function queuedMessageCount(dataset: FlowDataset, atMs: number): number {
	const queuedIds = new Set<string>();
	const settledIds = new Set<string>();
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'messages' || activity.atMs === null || activity.atMs > atMs) continue;
		const rowId = activity.source.row_id;
		if (activity.lifecycle === 'queued') queuedIds.add(rowId);
		else if (activity.lifecycle === 'delivered' || activity.lifecycle === 'consumed')
			settledIds.add(rowId);
	}
	let queued = 0;
	for (const rowId of queuedIds) if (!settledIds.has(rowId)) queued += 1;
	return queued;
}

export function buildCounts(dataset: FlowDataset, atMs: number): FlowCounts {
	const scene = sceneAt(dataset, atMs);
	const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
	const openCapturesByRole = new Map<FlowRole, number>();
	const agentsWithOpenCaptures = new Set<string>();
	let lessonInjectionsSoFar = 0;

	for (const activity of scene.activities) {
		const role = roles.get(activity.actor_id);
		if (role === undefined) continue;
		if (scene.openCaptureIds.has(activity.id)) {
			openCapturesByRole.set(role, (openCapturesByRole.get(role) ?? 0) + 1);
			if (role === 'agent') agentsWithOpenCaptures.add(activity.actor_id);
		}
		if (activity.kind === 'lesson_injections') lessonInjectionsSoFar += 1;
	}

	let errors = 0;
	for (const outcome of scene.outcomes.values()) if (outcome === 'error') errors += 1;

	return {
		atMs,
		openCapturesByRole,
		distinctAgentsWithOpenCaptures: agentsWithOpenCaptures.size,
		openToolCallsByFamily: openToolCallsByFamily(dataset, atMs, roles),
		errors,
		lessonInjectionsSoFar,
		messagesQueued: queuedMessageCount(dataset, atMs)
	};
}
