import { rolesOf } from './roles';
import type { FlowTimeScale } from './timeScale';
import type { FlowBounds, FlowDataset } from './types';

export const TRANSPORT_QUANTUM_MS = 30_000;

function clampToBounds(ms: number, bounds: FlowBounds): number {
	return Math.min(bounds.endMs, Math.max(bounds.startMs, ms));
}

function firstAbove(value: number, ascending: readonly number[]): number {
	let lo = 0,
		hi = ascending.length;
	while (lo < hi) {
		const mid = (lo + hi) >>> 1;
		if (ascending[mid]! <= value) lo = mid + 1;
		else hi = mid;
	}
	return lo;
}

function lastBelow(value: number, ascending: readonly number[]): number {
	let lo = 0,
		hi = ascending.length;
	while (lo < hi) {
		const mid = (lo + hi) >>> 1;
		if (ascending[mid]! < value) lo = mid + 1;
		else hi = mid;
	}
	return lo - 1;
}

export function buildTransportSteps(dataset: FlowDataset): readonly number[] {
	const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
	const steps = new Set<number>();
	for (const activity of dataset.activities.values()) {
		if (
			activity.kind !== 'requests' ||
			activity.time_semantics !== 'capture_interval' ||
			activity.atMs === null
		)
			continue;
		if (roles.get(activity.actor_id) !== 'coordinator') continue;
		steps.add(activity.atMs);
	}
	return [...steps].sort((a, b) => a - b);
}

export function nextTransportStepMs(
	steps: readonly number[],
	atMs: number | null,
	bounds: FlowBounds | null,
	scale: FlowTimeScale,
	speed: number
): number | null {
	if (bounds === null) return null;
	if (atMs === null) return bounds.endMs;
	if (atMs >= bounds.endMs) return bounds.endMs;
	for (let i = firstAbove(atMs, steps); i < steps.length; i++) {
		const step = steps[i]!;
		if (step > bounds.endMs) break;
		if (step >= bounds.startMs) return step;
	}
	return clampToBounds(scale.unwarp(scale.warp(atMs) + TRANSPORT_QUANTUM_MS * speed), bounds);
}

export function previousTransportStepMs(
	steps: readonly number[],
	atMs: number | null,
	bounds: FlowBounds | null,
	scale: FlowTimeScale,
	speed: number
): number | null {
	if (bounds === null) return null;
	if (atMs === null) return bounds.startMs;
	if (atMs <= bounds.startMs) return bounds.startMs;
	for (let i = lastBelow(atMs, steps); i >= 0; i--) {
		const step = steps[i]!;
		if (step < bounds.startMs) break;
		if (step <= bounds.endMs) return step;
	}
	return clampToBounds(scale.unwarp(scale.warp(atMs) - TRANSPORT_QUANTUM_MS * speed), bounds);
}
