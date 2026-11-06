import { describe, expect, it } from 'vitest';
import {
	FLEET_FIRST_LESSON_INJECTION_MS,
	FLEET_FIRST_PEER_MESSAGE_SENT_MS,
	fleetDataset,
	peakMs
} from './__fixtures__/fleet';
import { buildMarkers } from './markers';

const WIDTH = 1000;

describe('session-flow markers', () => {
	it('derives the first lesson injection, first peer message sent, and evidence bounds', () => {
		const markers = buildMarkers(fleetDataset(), WIDTH);
		const byId = new Map(markers.map((marker) => [marker.id, marker]));

		expect(byId.get('first-lesson-injection')!.atMs).toBe(FLEET_FIRST_LESSON_INJECTION_MS);
		expect(byId.get('first-lesson-injection')!.source).toBe('lesson_injection');

		expect(byId.get('first-peer-message-sent')!.atMs).toBe(FLEET_FIRST_PEER_MESSAGE_SENT_MS);
		expect(byId.get('first-peer-message-sent')!.source).toBe('peer_message_sent');

		const dataset = fleetDataset();
		expect(byId.get('evidence-start')!.atMs).toBe(dataset.bounds!.startMs);
		expect(byId.get('evidence-end')!.atMs).toBe(dataset.bounds!.endMs);
	});

	it("derives peak concurrent agents (21) and the last-dispatched resolved agent's first request", () => {
		const markers = buildMarkers(fleetDataset(), WIDTH);
		const byId = new Map(markers.map((marker) => [marker.id, marker]));

		const peak = byId.get('peak-concurrent-agents')!;
		expect(peak.atMs).toBe(peakMs);
		expect(peak.detail).toBe('21');

		const lastAgent = byId.get('last-agent-request')!;
		expect(lastAgent.atMs).toBe(peakMs);
		expect(lastAgent.source).toBe('last_agent_request');
	});

	it('bumps a colliding marker to a second row', () => {
		const markers = buildMarkers(fleetDataset(), WIDTH);
		const atPeak = markers.filter((marker) => marker.atMs === peakMs);
		expect(atPeak.length).toBeGreaterThanOrEqual(2);
		const rows = new Set(atPeak.map((marker) => marker.row));
		expect(rows.has(1)).toBe(true);
	});

	it('every marker is positioned within [0, width]', () => {
		for (const marker of buildMarkers(fleetDataset(), WIDTH)) {
			expect(marker.x).toBeGreaterThanOrEqual(0);
			expect(marker.x).toBeLessThanOrEqual(WIDTH);
		}
	});
});
