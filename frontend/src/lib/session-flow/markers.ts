import { buildAgentIdentities } from './grouping';
import { compareFlowIds } from './intervalIndex';
import { isMessageDelivery } from './messagePhases';
import { type FlowRole, rolesOf } from './roles';
import { buildTimeScale } from './timeScale';
import { flowTimeToX } from './timeline';
import type { FlowActivity, FlowDataset } from './types';

export type FlowMarkerSource =
	| 'lesson_injection'
	| 'peer_message_sent'
	| 'peak_agents'
	| 'last_agent_request'
	| 'evidence_start'
	| 'evidence_end';

export interface FlowMarker {
	id: string;
	label: string;
	atMs: number;
	source: FlowMarkerSource;
	detail: string | null;
	x: number;
	row: number;
}

const COLLISION_PITCH_PX = 40;
export const MAX_MARKER_ROWS = 3;
const LABEL_CHAR_PX = 6.6;

function labelWidthPx(label: string): number {
	return 8 + label.length * LABEL_CHAR_PX;
}

export function markerLabelFlips(x: number, label: string, width: number): boolean {
	return x + labelWidthPx(label) > width;
}

export function markerLabelExtent(
	x: number,
	label: string,
	width: number
): readonly [left: number, right: number] {
	const w = labelWidthPx(label);
	return markerLabelFlips(x, label, width) ? [x - w, x] : [x, x + w];
}

function firstActivityAtMs(
	dataset: FlowDataset,
	predicate: (activity: FlowActivity) => boolean
): number | null {
	let best: number | null = null;
	for (const activity of dataset.activities.values()) {
		if (activity.atMs === null || !predicate(activity)) continue;
		if (best === null || activity.atMs < best) best = activity.atMs;
	}
	return best;
}

function peakConcurrentAgents(
	dataset: FlowDataset,
	roles: ReadonlyMap<string, FlowRole>
): { atMs: number; count: number } | null {
	const events: { atMs: number; delta: number }[] = [];
	const rangeEndMs = dataset.bounds?.endMs ?? null;
	for (const activity of dataset.activities.values()) {
		if (activity.time_semantics !== 'capture_interval' || activity.atMs === null) continue;
		if (roles.get(activity.actor_id) !== 'agent') continue;
		events.push({ atMs: activity.atMs, delta: 1 });
		const validEnd =
			activity.endedAtMs !== null &&
			!activity.timeIssues.includes('end_before_start') &&
			!activity.timeIssues.includes('invalid_end');
		events.push({
			atMs: validEnd ? activity.endedAtMs! : (rangeEndMs ?? activity.atMs),
			delta: -1
		});
	}
	if (!events.length) return null;
	events.sort((a, b) => a.atMs - b.atMs || b.delta - a.delta);
	let count = 0,
		peak = 0,
		peakAtMs = events[0]!.atMs;
	for (const event of events) {
		count += event.delta;
		if (count > peak) {
			peak = count;
			peakAtMs = event.atMs;
		}
	}
	return peak > 0 ? { atMs: peakAtMs, count: peak } : null;
}

function lastAgentFirstRequestMarker(dataset: FlowDataset): FlowMarker | null {
	const identities = buildAgentIdentities(dataset).filter((identity) => !identity.pending);
	if (!identities.length) return null;
	const last = identities[identities.length - 1]!;
	let earliest: number | null = null;
	for (const actorId of last.actorIds) {
		for (const activity of dataset.activities.values()) {
			if (
				activity.actor_id !== actorId ||
				activity.kind !== 'requests' ||
				activity.time_semantics !== 'capture_interval' ||
				activity.atMs === null
			)
				continue;
			if (earliest === null || activity.atMs < earliest) earliest = activity.atMs;
		}
	}
	if (earliest === null) return null;
	return {
		id: 'last-agent-request',
		label: 'Last agent first request',
		atMs: earliest,
		source: 'last_agent_request',
		detail: last.label,
		x: 0,
		row: 0
	};
}

function candidateMarkers(
	dataset: FlowDataset,
	roles: ReadonlyMap<string, FlowRole>
): FlowMarker[] {
	const markers: FlowMarker[] = [];

	const lessonAtMs = firstActivityAtMs(
		dataset,
		(activity) => activity.kind === 'lesson_injections'
	);
	if (lessonAtMs !== null)
		markers.push({
			id: 'first-lesson-injection',
			label: 'First lesson injection',
			atMs: lessonAtMs,
			source: 'lesson_injection',
			detail: null,
			x: 0,
			row: 0
		});

	const peerSentAtMs = firstActivityAtMs(
		dataset,
		(activity) => isMessageDelivery(activity) && roles.get(activity.actor_id) === 'peer'
	);
	if (peerSentAtMs !== null)
		markers.push({
			id: 'first-peer-message-sent',
			label: 'First peer message sent',
			atMs: peerSentAtMs,
			source: 'peer_message_sent',
			detail: null,
			x: 0,
			row: 0
		});

	const peak = peakConcurrentAgents(dataset, roles);
	if (peak !== null)
		markers.push({
			id: 'peak-concurrent-agents',
			label: 'Peak concurrent agents',
			atMs: peak.atMs,
			source: 'peak_agents',
			detail: `${peak.count}`,
			x: 0,
			row: 0
		});

	const lastAgentMarker = lastAgentFirstRequestMarker(dataset);
	if (lastAgentMarker) markers.push(lastAgentMarker);

	if (dataset.bounds) {
		markers.push({
			id: 'evidence-start',
			label: 'Loaded activity start',
			atMs: dataset.bounds.startMs,
			source: 'evidence_start',
			detail: null,
			x: 0,
			row: 0
		});
		markers.push({
			id: 'evidence-end',
			label: 'Loaded activity end',
			atMs: dataset.bounds.endMs,
			source: 'evidence_end',
			detail: null,
			x: 0,
			row: 0
		});
	}
	return markers;
}

export function buildMarkers(dataset: FlowDataset, width: number): readonly FlowMarker[] {
	const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
	const range = dataset.bounds ?? { startMs: 0, endMs: 0 };
	const scale = buildTimeScale(dataset);
	const markers = candidateMarkers(dataset, roles)
		.map((marker) => ({ ...marker, x: flowTimeToX(marker.atMs, range, width, scale) }))
		.sort((a, b) => a.x - b.x || compareFlowIds(a.id, b.id));

	const lastRightByRow: number[] = [];
	return markers.map((marker) => {
		const [left, right] = markerLabelExtent(marker.x, marker.label, width);
		let row = lastRightByRow.findIndex((lastRight) => left - lastRight >= COLLISION_PITCH_PX);
		if (row === -1) {
			if (lastRightByRow.length < MAX_MARKER_ROWS) row = lastRightByRow.length;
			else row = lastRightByRow.indexOf(Math.min(...lastRightByRow));
		}
		lastRightByRow[row] = Math.max(lastRightByRow[row] ?? -Infinity, right);
		return { ...marker, row };
	});
}
