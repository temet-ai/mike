import type { FlowDataset } from './types';

export function buildRequestActivityIndex(dataset: FlowDataset): ReadonlyMap<string, string> {
	const index = new Map<string, string>();
	for (const [activityId, activity] of dataset.activities) {
		const requestId = activity.source.request_id;
		if (requestId !== null && !index.has(requestId)) index.set(requestId, activityId);
	}
	return index;
}

export function activityIdForRequest(
	index: ReadonlyMap<string, string>,
	requestId: number
): string | null {
	return index.get(String(requestId)) ?? null;
}
