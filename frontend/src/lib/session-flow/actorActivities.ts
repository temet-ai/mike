import type { FlowActivity, FlowDataset } from './types';

const EMPTY: readonly FlowActivity[] = [];
const CACHE = new WeakMap<FlowDataset, ReadonlyMap<string, readonly FlowActivity[]>>();

export function activitiesByActor(
	dataset: FlowDataset
): ReadonlyMap<string, readonly FlowActivity[]> {
	const cached = CACHE.get(dataset);
	if (cached) return cached;
	const byActor = new Map<string, FlowActivity[]>();
	for (const activity of dataset.activities.values()) {
		const list = byActor.get(activity.actor_id);
		if (list) list.push(activity);
		else byActor.set(activity.actor_id, [activity]);
	}
	CACHE.set(dataset, byActor);
	return byActor;
}

export function activitiesOfActor(dataset: FlowDataset, actorId: string): readonly FlowActivity[] {
	return activitiesByActor(dataset).get(actorId) ?? EMPTY;
}
