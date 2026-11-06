import { describe, expect, it } from 'vitest';
import { fleetDataset, peakMs } from './__fixtures__/fleet';
import { FLOW_START_MS } from './__fixtures__/sessionFlow';
import { buildCounts } from './counts';

describe('session-flow counts', () => {
	it('hand-computed: 500ms in, only the coordinator has opened its first request', () => {
		const counts = buildCounts(fleetDataset(), FLOW_START_MS + 500);
		expect([...counts.openCapturesByRole]).toEqual([['coordinator', 1]]);
		expect(counts.distinctAgentsWithOpenCaptures).toBe(0);
		expect([...counts.openToolCallsByFamily]).toEqual([]);
		expect(counts.errors).toBe(0);
		expect(counts.lessonInjectionsSoFar).toBe(0);
		expect(counts.messagesQueued).toBe(0);
	});

	it('hand-computed: 50s in, batches 1-2 (minus A6) are open, the late sidecar is still open', () => {
		const counts = buildCounts(fleetDataset(), FLOW_START_MS + 50_000);
		expect(counts.openCapturesByRole.get('agent')).toBe(9);
		expect(counts.openCapturesByRole.get('sidecars')).toBe(1);
		expect(counts.openCapturesByRole.has('coordinator')).toBe(false);
		expect(counts.distinctAgentsWithOpenCaptures).toBe(9);
		expect([...counts.openToolCallsByFamily]).toEqual([]);
		expect(counts.errors).toBe(1);
		expect(counts.lessonInjectionsSoFar).toBe(1);
		expect(counts.messagesQueued).toBe(4);
	});

	it('hand-computed: at peakMs, 21 agents and the 8 open tool calls by family', () => {
		const counts = buildCounts(fleetDataset(), peakMs);
		expect(counts.openCapturesByRole.get('agent')).toBe(21);
		expect(counts.distinctAgentsWithOpenCaptures).toBe(21);
		expect(new Map(counts.openToolCallsByFamily)).toEqual(
			new Map([
				['shell', 3],
				['files', 2],
				['browser', 1],
				['mike-mcp', 1],
				['github', 1]
			])
		);
		expect(counts.errors).toBe(1);
		expect(counts.lessonInjectionsSoFar).toBe(1);
		expect(counts.messagesQueued).toBe(4);
	});
});
