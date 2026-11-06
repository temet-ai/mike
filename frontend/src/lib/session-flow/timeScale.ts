import type { FlowDataset } from './types';

export const GAP_MIN_MS = 120_000;
export const GAP_SHOWN_MS = 30_000;

export interface FlowTimeGap {
	readonly startMs: number;
	readonly endMs: number;
	readonly shownMs: number;
}

export interface FlowTimeScale {
	readonly gaps: readonly FlowTimeGap[];
	warp(ms: number): number;
	unwarp(warpedMs: number): number;
}

export const LINEAR_TIME_SCALE: FlowTimeScale = Object.freeze({
	gaps: [],
	warp: (ms: number) => ms,
	unwarp: (ms: number) => ms
});

function busyPeriods(dataset: FlowDataset): Array<[number, number]> {
	const periods: Array<[number, number]> = [];
	for (const activity of dataset.activities.values()) {
		if (activity.atMs === null) continue;
		const endMs =
			activity.endedAtMs !== null && activity.endedAtMs > activity.atMs
				? activity.endedAtMs
				: activity.atMs;
		periods.push([activity.atMs, endMs]);
	}
	periods.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
	const merged: Array<[number, number]> = [];
	for (const [startMs, endMs] of periods) {
		const last = merged[merged.length - 1];
		if (last !== undefined && startMs <= last[1]) {
			if (endMs > last[1]) last[1] = endMs;
		} else merged.push([startMs, endMs]);
	}
	return merged;
}

function lastAtMost(value: number, ascending: readonly number[]): number {
	let lo = 0,
		hi = ascending.length;
	while (lo < hi) {
		const mid = (lo + hi) >>> 1;
		if (ascending[mid]! <= value) lo = mid + 1;
		else hi = mid;
	}
	return lo - 1;
}

export function buildTimeScaleFromPeriods(
	periods: ReadonlyArray<readonly [number, number]>,
	options: { minMs?: number; shownMs?: number } = {}
): FlowTimeScale {
	const minMs = options.minMs ?? GAP_MIN_MS;
	const shownMs = options.shownMs ?? GAP_SHOWN_MS;
	const gaps: FlowTimeGap[] = [];
	for (let i = 1; i < periods.length; i++) {
		const startMs = periods[i - 1]![1],
			endMs = periods[i]![0];
		if (endMs - startMs > minMs) gaps.push({ startMs, endMs, shownMs });
	}
	if (gaps.length === 0) return LINEAR_TIME_SCALE;
	const realStarts = gaps.map((gap) => gap.startMs);
	const warpStarts: number[] = [];
	const removedThrough: number[] = [];
	let removed = 0;
	for (const gap of gaps) {
		warpStarts.push(gap.startMs - removed);
		removed += gap.endMs - gap.startMs - gap.shownMs;
		removedThrough.push(removed);
	}
	return {
		gaps,
		warp(ms) {
			const i = lastAtMost(ms, realStarts);
			if (i < 0) return ms;
			const gap = gaps[i]!;
			if (ms >= gap.endMs) return ms - removedThrough[i]!;
			return warpStarts[i]! + ((ms - gap.startMs) * gap.shownMs) / (gap.endMs - gap.startMs);
		},
		unwarp(warpedMs) {
			const i = lastAtMost(warpedMs, warpStarts);
			if (i < 0) return warpedMs;
			const gap = gaps[i]!;
			const warpEnd = warpStarts[i]! + gap.shownMs;
			if (warpedMs >= warpEnd) return warpedMs + removedThrough[i]!;
			return gap.startMs + ((warpedMs - warpStarts[i]!) * (gap.endMs - gap.startMs)) / gap.shownMs;
		}
	};
}

const CACHE = new WeakMap<FlowDataset, FlowTimeScale>();

export function buildTimeScale(dataset: FlowDataset): FlowTimeScale {
	const cached = CACHE.get(dataset);
	if (cached) return cached;
	const scale = buildTimeScaleFromPeriods(busyPeriods(dataset));
	CACHE.set(dataset, scale);
	return scale;
}

export function formatGapDuration(ms: number): string {
	const totalSeconds = Math.round(ms / 1000);
	const hours = Math.floor(totalSeconds / 3600);
	const minutes = Math.floor((totalSeconds % 3600) / 60);
	const seconds = totalSeconds % 60;
	if (hours > 0) return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
	if (minutes > 0) return `${minutes}m`;
	return `${seconds}s`;
}
