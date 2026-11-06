import { compareFlowIds } from './intervalIndex';
import { chipFor, type MikeChip } from './serviceRoles';
import { type FlowRole, rolesOf } from './roles';
import { buildTimeScale } from './timeScale';
import { flowTimeToX } from './timeline';
import type { FlowDataset } from './types';

export type FlowLaneId = 'parent' | 'agents' | 'peer' | 'sidecars' | 'mike';

const LANE_ORDER: readonly FlowLaneId[] = ['parent', 'agents', 'peer', 'sidecars', 'mike'];
const LANE_LABEL: Readonly<Record<FlowLaneId, string>> = {
	parent: 'Parent',
	agents: 'Agents',
	peer: 'Peer',
	sidecars: 'Sidecars',
	mike: 'Mike'
};

export const LATE_DURATION_MS = 60_000;

export interface FlowLaneInterval {
	id: string;
	actorId: string;
	startMs: number;
	endMs: number | null;
	error: boolean;
	late: boolean;
	x: number;
	endX: number | null;
}

export interface FlowLaneTick {
	id: string;
	actorId: string;
	atMs: number;
	x: number;
	chip: MikeChip | null;
}

export interface FlowLane {
	id: FlowLaneId;
	label: string;
	intervals: readonly FlowLaneInterval[];
	ticks: readonly FlowLaneTick[];
}

function laneForRole(role: FlowRole): FlowLaneId | null {
	if (role === 'coordinator') return 'parent';
	if (role === 'agent') return 'agents';
	if (role === 'peer') return 'peer';
	if (role === 'sidecars') return 'sidecars';
	if (role.startsWith('service:')) return 'mike';
	return null;
}

function resolvedEndMs(activity: {
	endedAtMs: number | null;
	timeIssues: readonly string[];
}): number | null {
	if (activity.endedAtMs === null) return null;
	if (
		activity.timeIssues.includes('end_before_start') ||
		activity.timeIssues.includes('invalid_end')
	)
		return null;
	return activity.endedAtMs;
}

export function buildLaneModel(dataset: FlowDataset, width: number): readonly FlowLane[] {
	const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
	const range = dataset.bounds ?? { startMs: 0, endMs: 0 };
	const scale = buildTimeScale(dataset);
	const intervalsByLane = new Map<FlowLaneId, FlowLaneInterval[]>();
	const ticksByLane = new Map<FlowLaneId, FlowLaneTick[]>();
	for (const id of LANE_ORDER) {
		intervalsByLane.set(id, []);
		ticksByLane.set(id, []);
	}

	for (const activity of dataset.activities.values()) {
		if (activity.atMs === null) continue;
		const actor = dataset.actors.get(activity.actor_id);
		if (!actor) continue;
		const lane = laneForRole(roles.get(actor.id)!);
		if (lane === null) continue;
		if (activity.time_semantics === 'capture_interval') {
			const startMs = activity.atMs;
			const endMs = resolvedEndMs(activity);
			intervalsByLane.get(lane)!.push({
				id: activity.id,
				actorId: actor.id,
				startMs,
				endMs,
				error: activity.outcome === 'error',
				late: endMs !== null && endMs - startMs > LATE_DURATION_MS,
				x: flowTimeToX(startMs, range, width, scale),
				endX: endMs === null ? null : flowTimeToX(endMs, range, width, scale)
			});
		} else {
			ticksByLane.get(lane)!.push({
				id: activity.id,
				actorId: actor.id,
				atMs: activity.atMs,
				x: flowTimeToX(activity.atMs, range, width, scale),
				chip: chipFor(actor, activity)
			});
		}
	}

	return LANE_ORDER.map((id) => ({
		id,
		label: LANE_LABEL[id],
		intervals: intervalsByLane
			.get(id)!
			.sort((a, b) => a.startMs - b.startMs || compareFlowIds(a.id, b.id)),
		ticks: ticksByLane.get(id)!.sort((a, b) => a.atMs - b.atMs || compareFlowIds(a.id, b.id))
	}));
}
