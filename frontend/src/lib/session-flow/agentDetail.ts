import type { ToolUseDto } from '$lib/types';
import type { FlowDataset } from './types';

export type AgentLiveStatus = 'active' | 'idle' | 'finished' | 'failed' | 'not_matched';

export interface AgentRequestSpan {
	startMs: number | null;
	endMs: number | null;
	open: boolean;
}

export function agentRequestSpan(
	dataset: FlowDataset,
	actorIds: readonly string[]
): AgentRequestSpan {
	const wanted = new Set(actorIds);
	let startMs: number | null = null;
	let endMs: number | null = null;
	let open = false;
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'requests') continue;
		if (!wanted.has(activity.actor_id)) continue;
		if (activity.occurredAtMs !== null && (startMs === null || activity.occurredAtMs < startMs)) {
			startMs = activity.occurredAtMs;
		}
		if (activity.endedAtMs !== null && (endMs === null || activity.endedAtMs > endMs)) {
			endMs = activity.endedAtMs;
		}
		if (activity.lifecycle === 'started' && activity.endedAtMs === null) open = true;
	}
	return { startMs, endMs, open };
}

export function agentModels(dataset: FlowDataset, actorIds: readonly string[]): string[] {
	const wanted = new Set(actorIds);
	const models = new Set<string>();
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'requests') continue;
		if (!wanted.has(activity.actor_id)) continue;
		if (activity.metadata.model !== null) models.add(activity.metadata.model);
	}
	return [...models].sort();
}

export function agentCostUsd(dataset: FlowDataset, actorIds: readonly string[]): number | null {
	const wanted = new Set(actorIds);
	let total = 0;
	let any = false;
	for (const activity of dataset.activities.values()) {
		if (activity.kind !== 'requests') continue;
		if (!wanted.has(activity.actor_id)) continue;
		if (activity.metadata.cost_usd !== null) {
			total += activity.metadata.cost_usd;
			any = true;
		}
	}
	return any ? total : null;
}

export interface ToolCallCount {
	name: string;
	count: number;
}

export function toolCallCountsByName(
	rows: readonly ToolUseDto[],
	requestIds: ReadonlySet<number>
): ToolCallCount[] {
	const counts = new Map<string, number>();
	for (const row of rows) {
		if (!requestIds.has(row.request_id)) continue;
		counts.set(row.tool_name, (counts.get(row.tool_name) ?? 0) + 1);
	}
	return [...counts.entries()]
		.map(([name, count]) => ({ name, count }))
		.sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
}
