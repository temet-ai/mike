import { LINEAR_TIME_SCALE, type FlowTimeScale } from './timeScale';
import type { FlowActivity, FlowBounds } from './types';

export interface FlowTimelineMark {
	id: string;
	startMs: number;
	endMs: number;
	count: number;
	kind: 'point' | 'interval' | 'open';
}
export interface FlowTimelineLane {
	id: string;
	total: number;
	inWindow: number;
	unknown: number;
	marks: FlowTimelineMark[];
}
export function normalizeFlowRange(a: number, b: number): FlowBounds {
	if (!Number.isFinite(a) || !Number.isFinite(b)) throw new RangeError('Flow range must be finite');
	return { startMs: Math.min(a, b), endMs: Math.max(a, b) };
}
export function flowTimeToX(
	atMs: number,
	range: FlowBounds,
	width: number,
	scale: FlowTimeScale = LINEAR_TIME_SCALE
): number {
	if (!Number.isFinite(width) || width <= 0)
		throw new RangeError('Timeline width must be positive');
	if (!Number.isFinite(atMs)) throw new RangeError('Timeline time must be finite');
	const normalized = normalizeFlowRange(range.startMs, range.endMs);
	if (normalized.startMs === normalized.endMs) return width / 2;
	const start = scale.warp(normalized.startMs);
	const span = scale.warp(normalized.endMs) - start;
	return ((scale.warp(atMs) - start) / span) * width;
}
export function flowXToTime(
	x: number,
	range: FlowBounds,
	width: number,
	scale: FlowTimeScale = LINEAR_TIME_SCALE
): number {
	if (!Number.isFinite(x) || !Number.isFinite(width) || width <= 0)
		throw new RangeError('Timeline coordinates must be finite');
	const normalized = normalizeFlowRange(range.startMs, range.endMs);
	const start = scale.warp(normalized.startMs);
	const span = scale.warp(normalized.endMs) - start;
	return scale.unwarp(start + (Math.max(0, Math.min(width, x)) / width) * span);
}
export function flowKeyboardSeek(
	key: string,
	atMs: number,
	range: FlowBounds,
	stepMs: number
): number | null {
	const { startMs, endMs } = normalizeFlowRange(range.startMs, range.endMs);
	if (!Number.isFinite(atMs) || !Number.isFinite(stepMs) || stepMs <= 0)
		throw new RangeError('Seek values must be finite');
	if (key === 'Home') return startMs;
	if (key === 'End') return endMs;
	if (key !== 'ArrowLeft' && key !== 'ArrowRight') return null;
	return Math.max(startMs, Math.min(endMs, atMs + (key === 'ArrowLeft' ? -stepMs : stepMs)));
}

export function aggregateFlowTimeline(
	activities: Iterable<FlowActivity>,
	range: FlowBounds,
	width: number,
	groupBy: (activity: FlowActivity) => string = (activity) => activity.actor_id
): FlowTimelineLane[] {
	const window = normalizeFlowRange(range.startMs, range.endMs);
	if (!Number.isFinite(width) || width <= 0)
		throw new RangeError('Timeline width must be positive');
	const bins = Math.max(1, Math.min(128, Math.floor(width / 12)));
	const lanes = new Map<string, FlowTimelineLane>();
	const marks = new Map<string, Map<string, FlowTimelineMark>>();
	for (const activity of activities) {
		const id = groupBy(activity);
		let lane = lanes.get(id);
		if (!lane) {
			lane = { id, total: 0, inWindow: 0, unknown: 0, marks: [] };
			lanes.set(id, lane);
			marks.set(id, new Map());
		}
		lane.total += 1;
		const at = activity.atMs;
		if (at === null) {
			lane.unknown += 1;
			continue;
		}
		const capture = activity.time_semantics === 'capture_interval';
		const invalid =
			activity.timeIssues.includes('end_before_start') ||
			activity.timeIssues.includes('invalid_end');
		if (invalid) {
			lane.unknown += 1;
			continue;
		}
		const knownEnd = capture ? activity.endedAtMs : at;
		const end = knownEnd ?? at;
		if (at > window.endMs || end < window.startMs) continue;
		lane.inWindow += 1;
		const startMs = Math.max(window.startMs, at);
		const endMs = Math.min(window.endMs, end);
		const kind = capture && knownEnd === null ? 'open' : end === at ? 'point' : 'interval';
		const fraction =
			window.endMs === window.startMs
				? 0
				: (startMs - window.startMs) / (window.endMs - window.startMs);
		const bucket = Math.min(bins - 1, Math.floor(fraction * bins));
		const key = `${bucket}:${kind}`;
		const previous = marks.get(id)!.get(key);
		if (previous) {
			previous.count += 1;
			previous.startMs = Math.min(previous.startMs, startMs);
			previous.endMs = Math.max(previous.endMs, endMs);
		} else marks.get(id)!.set(key, { id: `${id}:${key}`, startMs, endMs, count: 1, kind });
	}
	for (const lane of lanes.values())
		lane.marks = [...marks.get(lane.id)!.values()].sort(
			(a, b) => a.startMs - b.startMs || a.id.localeCompare(b.id)
		);
	return [...lanes.values()].sort((a, b) => a.id.localeCompare(b.id));
}
