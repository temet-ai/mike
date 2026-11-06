import type { FlowActivity } from './types';

export interface FlowIndexEntry {
	id: string;
	atMs: number;
}
export interface FlowIntervalIndex {
	starts: readonly FlowIndexEntry[];
	ends: readonly FlowIndexEntry[];
	points: readonly FlowIndexEntry[];
	phases: readonly FlowIndexEntry[];
	unknownIds: readonly string[];
}
export function compareFlowIds(a: string, b: string): number {
	return a < b ? -1 : a > b ? 1 : 0;
}
function compareEntries(a: FlowIndexEntry, b: FlowIndexEntry): number {
	return a.atMs - b.atMs || compareFlowIds(a.id, b.id);
}
export function buildIntervalIndex(activities: Iterable<FlowActivity>): FlowIntervalIndex {
	const starts: FlowIndexEntry[] = [],
		ends: FlowIndexEntry[] = [],
		points: FlowIndexEntry[] = [],
		phases: FlowIndexEntry[] = [];
	const unknownIds: string[] = [];
	for (const activity of activities) {
		if (activity.atMs === null) {
			unknownIds.push(activity.id);
			continue;
		}
		const entry = { id: activity.id, atMs: activity.atMs };
		phases.push(entry);
		if (activity.time_semantics !== 'capture_interval') {
			points.push(entry);
			continue;
		}
		if (
			activity.timeIssues.includes('end_before_start') ||
			activity.timeIssues.includes('invalid_end')
		)
			continue;
		if (activity.endedAtMs === activity.atMs) {
			points.push(entry);
			continue;
		}
		starts.push(entry);
		if (activity.endedAtMs !== null) ends.push({ id: activity.id, atMs: activity.endedAtMs });
	}
	return {
		starts: starts.sort(compareEntries),
		ends: ends.sort(compareEntries),
		points: points.sort(compareEntries),
		phases: phases.sort(compareEntries),
		unknownIds: unknownIds.sort(compareFlowIds)
	};
}
export function entriesThrough(
	entries: readonly FlowIndexEntry[],
	atMs: number
): readonly FlowIndexEntry[] {
	let lo = 0,
		hi = entries.length;
	while (lo < hi) {
		const mid = (lo + hi) >>> 1;
		if (entries[mid]!.atMs <= atMs) lo = mid + 1;
		else hi = mid;
	}
	const prefix: FlowIndexEntry[] = [];
	for (let i = 0; i < lo; i++) prefix.push(entries[i]!);
	return prefix;
}
export function openCaptureIdsAt(index: FlowIntervalIndex, atMs: number): Set<string> {
	const ids = new Set(entriesThrough(index.starts, atMs).map((entry) => entry.id));
	for (const entry of entriesThrough(index.ends, atMs)) ids.delete(entry.id);
	return ids;
}
