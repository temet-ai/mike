import { describe, expect, it } from 'vitest';
import { routeBoard } from '$lib/board/router';
import type { ChipRect } from '$lib/board/types';
import {
	buildActorKeyMaps,
	buildBoardLayout,
	deriveConnections,
	MIKE_OUT_PAIRS,
	type BoardLayoutResult
} from './boardLayout';
import { FLEET_SESSION_ID, fleetDataset, peakMs } from './__fixtures__/fleet';
import {
	flowActor,
	flowActivity,
	flowCoverage,
	flowTime,
	FLOW_SESSION_ID
} from './__fixtures__/sessionFlow';
import { normalizeFlowDataset } from './projection';
import type { FlowDataset } from './types';

function layoutFor(
	dataset: FlowDataset,
	atMs: number,
	opts: { collapsed?: boolean; selected?: string | null } = {}
): BoardLayoutResult {
	const maps = buildActorKeyMaps(dataset);
	const conn = deriveConnections(dataset, atMs, maps);
	return buildBoardLayout(dataset, {
		stageW: 1372,
		collapsed: opts.collapsed ?? false,
		atMs,
		selected: opts.selected ?? null,
		calls: conn.calls,
		events: conn.events,
		mikeOut: MIKE_OUT_PAIRS
	});
}

type Rect = Pick<ChipRect, 'x' | 'y' | 'w' | 'h'>;
function rectsByKey(chips: BoardLayoutResult['chips']): Map<string, Rect> {
	return new Map(chips.map((c) => [c.key, { x: c.x, y: c.y, w: c.w, h: c.h }]));
}

function miniDataset(agentCount: number): FlowDataset {
	const coordId = `session:${FLOW_SESSION_ID}`;
	const actors = [
		flowActor(coordId, { kind: 'session', label: 'Coordinator', session_id: FLOW_SESSION_ID })
	];
	const activities = [];
	for (let i = 0; i < agentCount; i += 1) {
		const workerId = `worker:${FLOW_SESSION_ID}:${i}`;
		actors.push(flowActor(workerId, { kind: 'worker', label: `Agent ${i}` }));
		activities.push(
			flowActivity(`mini-req-${i}`, {
				actor_id: workerId,
				occurred_at: flowTime(1_000 + i),
				ended_at: null,
				lifecycle: 'started'
			})
		);
	}
	return normalizeFlowDataset({
		sessionId: FLOW_SESSION_ID,
		actors,
		activities,
		relationships: [],
		coverage: [flowCoverage()]
	});
}

describe('buildBoardLayout — position stability', () => {
	it('produces identical chip rects for the fleet dataset at two different atMs', () => {
		const early = layoutFor(fleetDataset(), 2_000);
		const late = layoutFor(fleetDataset(), peakMs);
		expect(rectsByKey(late.chips)).toEqual(rectsByKey(early.chips));
	});

	it('keeps every rect identical when only the selection changes', () => {
		const unselected = layoutFor(fleetDataset(), peakMs, { selected: null });
		const selected = layoutFor(fleetDataset(), peakMs, {
			selected: `worker:${FLEET_SESSION_ID}:instance:2`
		});
		expect(rectsByKey(selected.chips)).toEqual(rectsByKey(unselected.chips));
	});

	it('appends a chip and moves nothing else when an agent joins within the reserved 8-row grid', () => {
		const before = layoutFor(miniDataset(3), 5_000);
		const after = layoutFor(miniDataset(4), 5_000);
		expect(after.chips.length).toBe(before.chips.length + 1);
		const beforeRects = rectsByKey(before.chips);
		for (const chip of after.chips) {
			if (chip.key === 'w3') continue;
			expect(rectsByKey([chip]).get(chip.key), chip.key).toEqual(beforeRects.get(chip.key));
		}
	});

	it('reserves the fixed 3x8 grid capacity regardless of a smaller roster', () => {
		const small = layoutFor(miniDataset(3), 5_000);
		const full = layoutFor(fleetDataset(), peakMs);
		expect(small.H).toBe(full.H);
		expect(rectsByKey(small.chips).get('shell')).toEqual(rectsByKey(full.chips).get('shell'));
	});
});

describe('buildBoardLayout — fleet fixture content', () => {
	it('renders the full fixed topology: 4 outside + 15 service + 4 session + 24 agents + 5 tools', () => {
		const layout = layoutFor(fleetDataset(), peakMs);
		expect(layout.chips.length).toBe(4 + 15 + 4 + 24 + 5);
	});

	it('marks exactly 21 agents busy at peakMs (24 total minus 1 errored minus 2 pending)', () => {
		const layout = layoutFor(fleetDataset(), peakMs);
		const agentChips = layout.chips.filter((c) => /^w\d+$/.test(c.key));
		expect(agentChips).toHaveLength(24);
		expect(agentChips.filter((c) => c.state === 'busy')).toHaveLength(21);
		expect(agentChips.filter((c) => c.state === 'error')).toHaveLength(1);
		expect(agentChips.filter((c) => c.state === 'pending')).toHaveLength(2);
	});

	it('collapses to one group chip summarizing the same 21/1/2 split', () => {
		const layout = layoutFor(fleetDataset(), peakMs, { collapsed: true });
		const agentChips = layout.chips.filter((c) => /^w\d+$/.test(c.key));
		expect(agentChips).toHaveLength(0);
		const group = layout.chips.find((c) => c.key === 'agents')!;
		expect(group.sub).toBe('24 OF 24 · 21 IN FLIGHT · 1 ERROR · 2 IDLE');
	});

	it('produces a BoardInput the router accepts without throwing, expanded and collapsed', () => {
		expect(() =>
			routeBoard(layoutFor(fleetDataset(), peakMs, { collapsed: false }).input)
		).not.toThrow();
		expect(() =>
			routeBoard(layoutFor(fleetDataset(), peakMs, { collapsed: true }).input)
		).not.toThrow();
	});
});

describe('deriveConnections', () => {
	it('finds one call pair per calling agent and tool family, and the activity_request event pair, in the fleet fixture', () => {
		const dataset = fleetDataset();
		const maps = buildActorKeyMaps(dataset);
		const conn = deriveConnections(dataset, peakMs, maps);
		expect(conn.momentFallback).toBe(false);
		expect(conn.calls).toHaveLength(8);
		expect(conn.calls).toContainEqual(['w0', 'shell']);
		const perTool = new Map<string, number>();
		for (const [, tool] of conn.calls) perTool.set(tool, (perTool.get(tool) ?? 0) + 1);
		expect(Object.fromEntries(perTool)).toEqual({
			shell: 3,
			files: 2,
			browser: 1,
			mike: 1,
			xmcp: 1
		});
		expect(new Set(conn.calls.map(([agent]) => agent)).size).toBe(8);
		expect(conn.events.length).toBeGreaterThan(0);
	});
});
