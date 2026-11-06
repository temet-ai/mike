import { buildAgentIdentities } from './grouping';
import type { FlowDataset, FlowSelection } from './types';

export interface FlowRequestFilter {
	label: string;
	requestIds: ReadonlySet<number>;
}

function parseRequestId(raw: string | null): number | null {
	if (raw === null) return null;
	const n = Number(raw);
	return Number.isFinite(n) ? n : null;
}

export function requestIdsForActorIds(
	dataset: FlowDataset,
	actorIds: readonly string[]
): Set<number> {
	const wanted = new Set(actorIds);
	const ids = new Set<number>();
	if (wanted.size === 0) return ids;
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'requests') continue;
		if (!wanted.has(activity.actor_id)) continue;
		const rid = parseRequestId(activity.source.request_id);
		if (rid !== null) ids.add(rid);
	}
	return ids;
}

export function mainSessionActorId(dataset: FlowDataset): string | null {
	for (const actor of dataset.actors.values()) {
		if (actor.kind === 'session' && actor.session_id === dataset.sessionId) return actor.id;
	}
	return null;
}

export function buildFlowRequestFilter(
	dataset: FlowDataset,
	selection: FlowSelection
): FlowRequestFilter | null {
	if (selection === null) return null;
	if (selection.kind === 'activity') {
		const activity = dataset.activities.get(selection.id);
		if (!activity) return null;
		const rid = parseRequestId(activity.source.request_id);
		if (rid === null) return null;
		const actor = dataset.actors.get(activity.actor_id);
		return {
			label: `${actor?.label ?? 'selected activity'} · ${activity.kind.replaceAll('_', ' ')}`,
			requestIds: new Set([rid])
		};
	}
	if (selection.kind === 'actor') {
		const coordinatorId = mainSessionActorId(dataset);
		if (selection.id === coordinatorId) {
			return { label: 'main session', requestIds: requestIdsForActorIds(dataset, [selection.id]) };
		}
		const identity = buildAgentIdentities(dataset).find((entry) =>
			entry.actorIds.includes(selection.id)
		);
		if (identity) {
			return {
				label: identity.label,
				requestIds: requestIdsForActorIds(dataset, identity.workerActorIds)
			};
		}
		return null;
	}
	return null;
}
