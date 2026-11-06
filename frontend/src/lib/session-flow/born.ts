import { activitiesByActor } from './actorActivities';
import { compareFlowIds } from './intervalIndex';
import type { FlowActivity, FlowDataset } from './types';

interface FirstTimes {
	started: number | null;
	observed: number | null;
	any: number | null;
}

const BORN_CACHE = new WeakMap<FlowDataset, ReadonlyMap<string, number | null>>();

function firstTimesOf(activities: readonly FlowActivity[] | undefined): FirstTimes {
	const first: FirstTimes = { started: null, observed: null, any: null };
	for (const activity of activities ?? []) {
		if (activity.atMs === null) continue;
		if (first.any === null || activity.atMs < first.any) first.any = activity.atMs;
		if (activity.phase === 'started' && (first.started === null || activity.atMs < first.started))
			first.started = activity.atMs;
		if (
			activity.phase === 'observed' &&
			(first.observed === null || activity.atMs < first.observed)
		)
			first.observed = activity.atMs;
	}
	return first;
}

function earliest(...candidates: readonly (number | null)[]): number | null {
	let min: number | null = null;
	for (const candidate of candidates)
		if (candidate !== null && (min === null || candidate < min)) min = candidate;
	return min;
}

export function bornIndex(dataset: FlowDataset): ReadonlyMap<string, number | null> {
	const cached = BORN_CACHE.get(dataset);
	if (cached) return cached;
	const byActor = activitiesByActor(dataset);
	const firstTimes = new Map<string, FirstTimes>();
	const timesOf = (actorId: string): FirstTimes => {
		let times = firstTimes.get(actorId);
		if (!times) {
			times = firstTimesOf(byActor.get(actorId));
			firstTimes.set(actorId, times);
		}
		return times;
	};
	const index = new Map<string, number | null>();
	for (const actor of dataset.actors.values()) {
		const own = timesOf(actor.id);
		if (actor.kind === 'worker')
			index.set(
				actor.id,
				earliest(
					actor.parent_actor_id !== null ? timesOf(actor.parent_actor_id).observed : null,
					own.started
				)
			);
		else if (actor.kind === 'unresolved_dispatch')
			index.set(actor.id, earliest(own.observed, own.started));
		else index.set(actor.id, own.any);
	}
	BORN_CACHE.set(dataset, index);
	return index;
}

export function bornAtMs(dataset: FlowDataset, actorId: string): number | null {
	return bornIndex(dataset).get(actorId) ?? null;
}

export function compareBornAtMs(
	dataset: FlowDataset
): (actorIdA: string, actorIdB: string) => number {
	return (actorIdA, actorIdB) => {
		const a = bornAtMs(dataset, actorIdA);
		const b = bornAtMs(dataset, actorIdB);
		if (a === null && b === null) return compareFlowIds(actorIdA, actorIdB);
		if (a === null) return 1;
		if (b === null) return -1;
		return a - b || compareFlowIds(actorIdA, actorIdB);
	};
}
