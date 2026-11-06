import type {
	FlowBounds,
	FlowDataset,
	FlowPlayback,
	FlowPlaybackMode,
	FlowScheduler,
	FlowSeekOptions,
	FlowSeekResult
} from './types';
import { buildTimeScale } from './timeScale';

const browserScheduler: FlowScheduler = {
	now: () => performance.now(),
	requestFrame: (callback) => requestAnimationFrame(callback),
	cancelFrame: (handle) => cancelAnimationFrame(handle)
};

export function createFlowPlayback(options: {
	dataset: FlowDataset;
	scheduler?: FlowScheduler;
	initialMode?: FlowPlaybackMode;
	initialAt?: 'start' | 'end';
}): FlowPlayback {
	let dataset = options.dataset;
	let scale = buildTimeScale(dataset);
	const scheduler = options.scheduler ?? browserScheduler;
	const initialMode = options.initialMode ?? 'paused';
	const initialAt = options.initialAt ?? 'start';
	const restingAtMs = (bounds: FlowBounds | null): number | null =>
		bounds === null ? null : initialAt === 'end' ? bounds.endMs : bounds.startMs;
	const initialAtMs =
		initialMode === 'live' ? dataset.observationFrontierMs : restingAtMs(dataset.bounds);
	let mode = $state<FlowPlaybackMode>(initialMode);
	let atMs = $state<number | null>(initialAtMs);
	let userMoved = false;
	let speed = $state(1);
	let seekResult = $state<FlowSeekResult | null>(null);
	let anchorAt = initialAtMs,
		anchorNow = scheduler.now();
	let frame: number | null = null,
		disposed = false;
	function cancel(): void {
		if (frame !== null) scheduler.cancelFrame(frame);
		frame = null;
	}
	function anchor(): void {
		anchorAt = atMs;
		anchorNow = scheduler.now();
	}
	function sample(): void {
		if (mode !== 'playing' || anchorAt === null || dataset.bounds === null) return;
		atMs = Math.min(
			dataset.bounds.endMs,
			scale.unwarp(scale.warp(anchorAt) + Math.max(0, scheduler.now() - anchorNow) * speed)
		);
		if (atMs >= dataset.bounds.endMs) {
			mode = 'paused';
			cancel();
		}
	}
	function schedule(): void {
		if (disposed || mode !== 'playing' || frame !== null) return;
		frame = scheduler.requestFrame(() => {
			frame = null;
			if (disposed) return;
			sample();
			schedule();
		});
	}
	function pause(): void {
		if (disposed) return;
		sample();
		mode = 'paused';
		cancel();
		anchor();
	}
	function positionWhilePlaying(requestedAtMs: number): void {
		atMs = requestedAtMs;
		anchor();
		schedule();
		if (dataset.bounds !== null && atMs >= dataset.bounds.endMs) {
			mode = 'paused';
			cancel();
		}
	}
	function seek(requestedAtMs: number, options?: FlowSeekOptions): FlowSeekResult {
		userMoved = true;
		if (
			disposed ||
			!Number.isFinite(requestedAtMs) ||
			dataset.bounds === null ||
			requestedAtMs < dataset.bounds.startMs ||
			requestedAtMs > dataset.bounds.endMs
		) {
			const result: FlowSeekResult = {
				status: 'unavailable',
				requestedAtMs,
				bounds: dataset.bounds
			};
			if (!disposed) {
				pause();
				seekResult = result;
			}
			return result;
		}
		if (options?.keepPlaying && mode === 'playing') positionWhilePlaying(requestedAtMs);
		else {
			pause();
			atMs = requestedAtMs;
			anchor();
		}
		seekResult = { status: 'positioned', atMs: requestedAtMs };
		return seekResult;
	}
	if (initialMode === 'playing') {
		if (dataset.bounds === null) mode = 'paused';
		else schedule();
	}
	return {
		get atMs() {
			return atMs;
		},
		get mode() {
			return mode;
		},
		get speed() {
			return speed;
		},
		get seekResult() {
			return seekResult;
		},
		setDataset(next) {
			if (disposed) return;
			if (next.sessionId !== dataset.sessionId)
				throw new Error('Create a new Flow playback for a different session');
			sample();
			dataset = next;
			scale = buildTimeScale(dataset);
			if (mode === 'playing' && dataset.bounds === null) {
				mode = 'paused';
				cancel();
			}
			if (mode === 'live') atMs = dataset.observationFrontierMs;
			if (mode === 'paused' && (atMs === null || (!userMoved && initialAt === 'end')))
				atMs = restingAtMs(dataset.bounds);
			anchor();
			schedule();
		},
		play() {
			if (disposed || dataset.bounds === null) return;
			userMoved = true;
			if (atMs === null || atMs >= dataset.bounds.endMs || atMs < dataset.bounds.startMs)
				atMs = dataset.bounds.startMs;
			mode = 'playing';
			anchor();
			sample();
			schedule();
		},
		pause,
		seek,
		restart() {
			if (!disposed && dataset.bounds !== null) seek(dataset.bounds.startMs);
		},
		end() {
			if (!disposed && dataset.bounds !== null) seek(dataset.bounds.endMs);
		},
		returnToLive() {
			if (disposed) return;
			cancel();
			mode = 'live';
			atMs = dataset.observationFrontierMs;
			seekResult = null;
			anchor();
		},
		setSpeed(next) {
			if (disposed) return;
			if (!Number.isFinite(next) || next <= 0)
				throw new RangeError('Flow playback speed must be positive');
			sample();
			speed = next;
			anchor();
			schedule();
		},
		resume() {
			if (disposed) return;
			sample();
			anchor();
			schedule();
		},
		dispose() {
			if (disposed) return;
			cancel();
			disposed = true;
			mode = 'paused';
		}
	};
}
