import type { SessionFlowActivity } from '$lib/api/generated/SessionFlowActivity';
import {
	buildIntervalIndex,
	compareFlowIds,
	entriesThrough,
	openCaptureIdsAt
} from './intervalIndex';
import type {
	FlowActivity,
	FlowDataset,
	FlowDatasetInput,
	FlowScene,
	FlowTimeIssue
} from './types';

export function flowTimestamp(value: string | null): number | null {
	if (value === null || !/(?:Z|[+-]\d{2}:\d{2})$/i.test(value)) return null;
	const parsed = Date.parse(value);
	return Number.isFinite(parsed) ? parsed : null;
}
export function preferFlowActivity(
	previous: SessionFlowActivity | undefined,
	incoming: SessionFlowActivity,
	compareRevision?: (table: string, incoming: string, previous: string) => number | null
): SessionFlowActivity {
	if (!previous || previous.id !== incoming.id) return incoming;
	if (previous.source_revision !== null && incoming.source_revision !== null) {
		if (previous.source_revision === incoming.source_revision) return previous;
		const comparison = compareRevision?.(
			incoming.source.table,
			incoming.source_revision,
			previous.source_revision
		);
		if (comparison !== undefined && comparison !== null && comparison < 0) return previous;
	}
	return incoming;
}
function normalizeActivity(activity: SessionFlowActivity): FlowActivity {
	const occurredAtMs = flowTimestamp(activity.occurred_at),
		recordedAtMs = flowTimestamp(activity.recorded_at),
		endedAtMs = flowTimestamp(activity.ended_at);
	const timeIssues: FlowTimeIssue[] = [];
	if (occurredAtMs === null)
		timeIssues.push(activity.occurred_at === null ? 'unknown_occurrence' : 'invalid_occurrence');
	if (recordedAtMs === null) timeIssues.push('invalid_recorded');
	if (activity.ended_at !== null && endedAtMs === null) timeIssues.push('invalid_end');
	if (endedAtMs !== null && occurredAtMs !== null && endedAtMs < occurredAtMs)
		timeIssues.push('end_before_start');
	const observed =
		activity.time_semantics === 'recorded_observation' ||
		activity.time_semantics === 'stored_output' ||
		activity.time_semantics === 'measured_latency';
	const atMs = observed ? (occurredAtMs ?? recordedAtMs) : occurredAtMs;
	return { ...activity, occurredAtMs, recordedAtMs, endedAtMs, atMs, timeIssues };
}
export function normalizeFlowDataset(input: FlowDatasetInput): FlowDataset {
	const actors = new Map((input.actors ?? []).map((actor) => [actor.id, actor]));
	const activities = new Map<string, FlowActivity>();
	for (const activity of input.activities ?? [])
		activities.set(
			activity.id,
			normalizeActivity(preferFlowActivity(activities.get(activity.id), activity))
		);
	const relationships = new Map(
		(input.relationships ?? []).map((relationship) => {
			const occurredAtMs = flowTimestamp(relationship.occurred_at),
				recordedAtMs = flowTimestamp(relationship.recorded_at);
			return [
				relationship.id,
				{ ...relationship, occurredAtMs, recordedAtMs, atMs: occurredAtMs ?? recordedAtMs }
			];
		})
	);
	const coverage = new Map((input.coverage ?? []).map((item) => [item.component, item]));
	const observations = [...coverage.values()].flatMap((item) => {
		const ms = flowTimestamp(item.server_observed_at);
		return ms === null ? [] : [ms];
	});
	const times: number[] = [];
	for (const item of activities.values()) {
		if (item.atMs !== null) times.push(item.atMs);
		if (
			item.time_semantics === 'capture_interval' &&
			item.endedAtMs !== null &&
			!item.timeIssues.includes('end_before_start') &&
			item.atMs !== null
		)
			times.push(item.endedAtMs);
	}
	for (const item of relationships.values()) if (item.atMs !== null) times.push(item.atMs);
	return {
		sessionId: input.sessionId,
		actors,
		activities,
		relationships,
		coverage,
		observationFrontierMs: observations.length ? Math.max(...observations) : null,
		bounds: times.length ? { startMs: Math.min(...times), endMs: Math.max(...times) } : null,
		index: buildIntervalIndex(activities.values())
	};
}
function phaseAt(activity: FlowActivity, atMs: number): FlowActivity {
	if (activity.time_semantics !== 'capture_interval') return activity;
	const ended =
		activity.endedAtMs !== null &&
		activity.endedAtMs >= (activity.atMs ?? Infinity) &&
		activity.endedAtMs <= atMs;
	return {
		...activity,
		ended_at: ended ? activity.ended_at : null,
		endedAtMs: ended ? activity.endedAtMs : null,
		outcome: ended ? activity.outcome : null,
		lifecycle: ended ? activity.lifecycle : 'started',
		metadata: ended
			? activity.metadata
			: {
					...activity.metadata,
					status_code: null,
					verdict: null,
					has_verdict: null,
					output_tokens: null,
					cost_usd: null,
					latency_ms: null
				}
	};
}
export function sceneAt(dataset: FlowDataset, atMs: number): FlowScene {
	if (!Number.isFinite(atMs)) throw new RangeError('Flow playhead must be finite');
	const activities = entriesThrough(dataset.index.phases, atMs).map(({ id }) =>
		phaseAt(dataset.activities.get(id)!, atMs)
	);
	const relationships = [...dataset.relationships.values()]
		.filter((item) => item.atMs !== null && item.atMs <= atMs)
		.sort((a, b) => a.atMs! - b.atMs! || compareFlowIds(a.id, b.id));
	const actorIds = new Set(activities.map((activity) => activity.actor_id));
	for (const relationship of relationships) {
		for (const id of [relationship.from_id, relationship.to_id])
			if (id !== null && dataset.actors.has(id)) actorIds.add(id);
	}
	const actors = [...actorIds].sort(compareFlowIds).flatMap((id) => {
		const actor = dataset.actors.get(id);
		return actor ? [actor] : [];
	});
	const openCaptureIds = openCaptureIdsAt(dataset.index, atMs);
	const pointIds = new Set(entriesThrough(dataset.index.points, atMs).map((entry) => entry.id));
	const outcomes = new Map(
		activities.flatMap((activity) =>
			activity.outcome === null ? [] : [[activity.id, activity.outcome] as const]
		)
	);
	return {
		atMs,
		actors,
		activities,
		relationships,
		openCaptureIds,
		pointIds,
		outcomes,
		unknownActivityIds: dataset.index.unknownIds,
		counts: {
			actors: actors.length,
			activities: activities.length,
			openCaptures: openCaptureIds.size,
			points: pointIds.size,
			outcomes: outcomes.size
		},
		coverage: dataset.coverage
	};
}
