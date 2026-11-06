import { describe, expect, it } from 'vitest';
import {
	FLEET_A2_ACTOR_ID,
	FLEET_COORDINATOR_ID,
	FLEET_TOOL_ACTOR_IDS,
	fleetDataset,
	peakMs
} from './__fixtures__/fleet';
import { buildTraceModel } from './traceModel';

describe('session-flow traceModel', () => {
	it('gives 21 open captures and 8 open tool calls at peakMs', () => {
		const entries = buildTraceModel(fleetDataset(), peakMs, FLEET_A2_ACTOR_ID);
		expect(entries.filter((entry) => entry.kind === 'capture')).toHaveLength(21);

		const toolcalls = entries.filter((entry) => entry.kind === 'toolcall');
		expect(toolcalls).toHaveLength(8);
		const toolcallActorIds = new Set(toolcalls.map((entry) => entry.from));
		for (const id of FLEET_TOOL_ACTOR_IDS.shell) expect(toolcallActorIds.has(id)).toBe(true);
		for (const id of FLEET_TOOL_ACTOR_IDS.files) expect(toolcallActorIds.has(id)).toBe(true);
		for (const id of FLEET_TOOL_ACTOR_IDS.browser) expect(toolcallActorIds.has(id)).toBe(true);
		expect(toolcallActorIds.has(FLEET_TOOL_ACTOR_IDS.mikeMcp)).toBe(true);
		expect(toolcallActorIds.has(FLEET_TOOL_ACTOR_IDS.githubMcp)).toBe(true);

		const internet = entries.filter((entry) => entry.kind === 'internet');
		expect(internet).toHaveLength(2);
		const internetFrom = new Set(internet.map((entry) => entry.from));
		expect(internetFrom.has(FLEET_TOOL_ACTOR_IDS.browser[0])).toBe(true);
		expect(internetFrom.has(FLEET_TOOL_ACTOR_IDS.githubMcp)).toBe(true);
	});

	it('gives the dispatch trace for the selected agent', () => {
		const entries = buildTraceModel(fleetDataset(), peakMs, FLEET_A2_ACTOR_ID);
		const dispatchToA2 = entries.find(
			(entry) => entry.kind === 'dispatch' && entry.to === FLEET_A2_ACTOR_ID
		);
		expect(dispatchToA2).toBeDefined();
		expect(dispatchToA2!.from).toBe(FLEET_COORDINATOR_ID);
		expect(dispatchToA2!.opacity).toBe(1);

		const withoutSelection = buildTraceModel(fleetDataset(), peakMs, null);
		expect(
			withoutSelection.some((entry) => entry.kind === 'dispatch' && entry.to === FLEET_A2_ACTOR_ID)
		).toBe(false);
	});

	it('gives the firing-chip event that reaches the coordinator', () => {
		const entries = buildTraceModel(fleetDataset(), peakMs, null);
		const coordinatorEvent = entries.find(
			(entry) =>
				(entry.kind === 'mikeout' || entry.kind === 'audit') && entry.to === FLEET_COORDINATOR_ID
		);
		expect(coordinatorEvent).toBeDefined();
		expect(coordinatorEvent!.kind).toBe('mikeout');
	});

	it('carries a 0..1 packet phase and never a negative opacity', () => {
		for (const entry of buildTraceModel(fleetDataset(), peakMs, FLEET_A2_ACTOR_ID)) {
			expect(entry.phase).toBeGreaterThanOrEqual(0);
			expect(entry.phase).toBeLessThan(1);
			expect(entry.opacity).toBeGreaterThanOrEqual(0);
			expect(entry.opacity).toBeLessThanOrEqual(1);
		}
	});
});
