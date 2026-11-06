import { describe, expect, it } from 'vitest';
import {
	FLOW_START_MS as start,
	flowActivity,
	flowDataset,
	flowTime
} from './__fixtures__/sessionFlow';
import { createFlowPlayback } from './playback.svelte';
import type { FlowScheduler } from './types';

function fakeScheduler() {
	let now = 0,
		nextId = 0;
	const frames = new Map<number, () => void>();
	const scheduler: FlowScheduler = {
		now: () => now,
		requestFrame: (callback) => {
			const id = ++nextId;
			frames.set(id, callback);
			return id;
		},
		cancelFrame: (id) => {
			frames.delete(id);
		}
	};
	return {
		scheduler,
		frames,
		advance(ms: number, render = true) {
			now += ms;
			if (render) {
				const pending = [...frames.values()];
				frames.clear();
				for (const callback of pending) callback();
			}
		}
	};
}

describe('session-flow production playback', () => {
	it('uses one absolute clock and reanchors speed without jumps', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.play();
		fake.advance(500);
		expect(playback.atMs).toBe(start + 500);
		playback.setSpeed(2);
		fake.advance(500);
		expect(playback.atMs).toBe(start + 1500);
		playback.pause();
		fake.advance(500);
		expect(playback.atMs).toBe(start + 1500);
		playback.seek(start + 100);
		playback.play();
		fake.advance(100);
		expect(playback.atMs).toBe(start + 300);
		playback.dispose();
	});
	it('samples background elapsed once without accumulating frames', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.play();
		fake.advance(2000, false);
		playback.resume();
		expect(playback.atMs).toBe(start + 2000);
		expect(fake.frames.size).toBe(1);
		fake.advance(100);
		expect(playback.atMs).toBe(start + 2100);
		playback.dispose();
	});
	it('accepts data while paused and follows only the supplied frontier in live mode', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.seek(start + 500);
		playback.setDataset(
			flowDataset([
				flowActivity('later', { occurred_at: flowTime(1000), ended_at: flowTime(10000) })
			])
		);
		expect(playback.atMs).toBe(start + 500);
		playback.returnToLive();
		expect(playback.atMs).toBe(start + 4000);
		fake.advance(100000);
		expect(playback.atMs).toBe(start + 4000);
		playback.dispose();
	});
	it('reports unavailable restored seeks without silently clamping', () => {
		const playback = createFlowPlayback({
			dataset: flowDataset(),
			scheduler: fakeScheduler().scheduler
		});
		expect(playback.seek(start - 100)).toMatchObject({
			status: 'unavailable',
			requestedAtMs: start - 100
		});
		expect(playback.atMs).toBe(start);
		playback.end();
		expect(playback.atMs).toBe(start + 4000);
		playback.restart();
		expect(playback.atMs).toBe(start);
		playback.dispose();
	});
	it('has no seek range for empty eligible data', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset([]), scheduler: fake.scheduler });
		expect(playback.atMs).toBeNull();
		playback.play();
		expect(fake.frames.size).toBe(0);
		expect(playback.seek(start)).toMatchObject({ status: 'unavailable', bounds: null });
		playback.dispose();
	});
	it('stops at loaded end and disposes outstanding frame work idempotently', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.play();
		fake.advance(10000);
		expect(playback.mode).toBe('paused');
		expect(playback.atMs).toBe(start + 4000);
		expect(fake.frames.size).toBe(0);
		playback.restart();
		playback.play();
		playback.dispose();
		playback.dispose();
		fake.advance(1000);
		expect(fake.frames.size).toBe(0);
		expect(playback.atMs).toBe(start);
	});
	it('keeps playing through a transport seek and resumes advancing from the new position', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.play();
		fake.advance(500);
		const result = playback.seek(start + 1000, { keepPlaying: true });
		expect(playback.mode).toBe('playing');
		expect(playback.atMs).toBe(start + 1000);
		expect(result).toMatchObject({ status: 'positioned', atMs: start + 1000 });
		fake.advance(1000);
		expect(playback.atMs).toBe(start + 2000);
		playback.dispose();
	});
	it('still pauses on a plain seek while playing when keepPlaying is omitted', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.play();
		fake.advance(500);
		playback.seek(start + 1000);
		expect(playback.mode).toBe('paused');
		expect(playback.atMs).toBe(start + 1000);
		playback.dispose();
	});
	it('stays paused when a keepPlaying seek arrives while already paused', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.seek(start + 1000, { keepPlaying: true });
		expect(playback.mode).toBe('paused');
		expect(playback.atMs).toBe(start + 1000);
		playback.dispose();
	});
	it('leaves live and pauses on a keepPlaying seek from live mode', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.returnToLive();
		playback.seek(start + 1000, { keepPlaying: true });
		expect(playback.mode).toBe('paused');
		expect(playback.atMs).toBe(start + 1000);
		playback.dispose();
	});
	it('ends paused at the loaded end even when keepPlaying is requested', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.play();
		const result = playback.seek(start + 4000, { keepPlaying: true });
		expect(playback.mode).toBe('paused');
		expect(playback.atMs).toBe(start + 4000);
		expect(result).toMatchObject({ status: 'positioned', atMs: start + 4000 });
		playback.dispose();
	});
	it('still reports unavailable and pauses on an out-of-bounds seek with keepPlaying', () => {
		const fake = fakeScheduler(),
			playback = createFlowPlayback({ dataset: flowDataset(), scheduler: fake.scheduler });
		playback.play();
		fake.advance(500);
		const result = playback.seek(start + 100000, { keepPlaying: true });
		expect(result).toMatchObject({ status: 'unavailable', requestedAtMs: start + 100000 });
		expect(playback.mode).toBe('paused');
		playback.dispose();
	});
});
