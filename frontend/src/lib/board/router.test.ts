import { describe, it, expect } from 'vitest';
import { entersRect, tooClose } from './geometry';
import { routeBoard, type BoardInput, type BoardRoutes } from './router';
import { tracePath } from './trace';
import { PITCH, type ChipRect, type Point } from './types';

const STRIP_X = 926;
const STRIP_W = 230;
const SPINE_REQ = STRIP_X + 10;
const SPINE_RES = STRIP_X + 117;
const SPINE_EVT = STRIP_X + 4;
const SPINE_REQ_R = STRIP_X + STRIP_W - 8;
const SPINE_RES_R = STRIP_X + STRIP_W - 11;
const FLOOR_REQ = 349;
const FLOOR_RES = 346;
const OUT_X = 1180;
const OUT_W = 176;
const EVT_Y = 59;

const COLS = 3;
const ROWS = 8;
const PITCH_X = 192;
const PITCH_Y = 66;
const CH = 40;
const CW = 150;
const A_X = 264;
const A_W = 574;
const AX0 = A_X + 28;
const AY0 = 100;

interface Fixture {
	input: BoardInput;
	chipsByKey: Map<string, ChipRect>;
}

function buildFixture(collapsed: boolean, callsOverride?: Array<[string, string]>): Fixture {
	const chips: ChipRect[] = [];
	const push = (
		key: string,
		kind: ChipRect['kind'],
		x: number,
		y: number,
		w: number,
		h: number
	): void => {
		chips.push({ key, kind, x, y, w, h });
	};

	push('model', 'outside', OUT_X, 44, OUT_W, 48);
	push('openrouter', 'outside', OUT_X, 172, OUT_W, 48);
	push('ollama', 'outside', OUT_X, 300, OUT_W, 48);
	push('internet', 'outside', OUT_X, 428, OUT_W, 48);

	const REQ_X = STRIP_X + 22;
	const RES_X = STRIP_X + 120;
	const BG_X = STRIP_X + 71;
	const requestPath = ['r_capture', 'filters', 'composer', 'inject', 'compress'];
	const responsePath = ['s_capture', 'intercept', 'analyzers', 'extract', 'persist'];
	const background = ['judge', 'verifier', 'embed', 'mailbox', 'upkeep'];
	requestPath.forEach((k, i) => push(k, 'service', REQ_X, 100 + i * 52, 92, 28));
	responsePath.forEach((k, i) => push(k, 'service', RES_X, 100 + i * 52, 92, 28));
	background.forEach((k, i) => push(k, 'service', BG_X, 380 + i * 48, 92, 28));

	push('coord', 'session', 56, 100, 184, 56);
	push('peer', 'peer', 56, 172, 184, 48);
	push('side', 'group', 56, 236, 184, 48);
	push('term', 'terminal', 56, 324, 184, 36);

	let gridBottom: number;
	if (collapsed) {
		push('agents', 'group', 280, 100, 542, 40);
		gridBottom = AY0 + 40;
	} else {
		for (let i = 0; i < 24; i++) {
			const c = i % COLS;
			const r = Math.floor(i / COLS);
			push(`w${i}`, 'agent', AX0 + c * PITCH_X, AY0 + r * PITCH_Y, CW, CH);
		}
		gridBottom = AY0 + (ROWS - 1) * PITCH_Y + CH;
	}
	const toolHeadY = gridBottom + 22;
	const bandY0 = toolHeadY + 16;
	const toolsY = bandY0 + 8 * PITCH + 10;
	push('shell', 'tool', AX0 - 12, toolsY, 160, 40);
	push('files', 'tool', AX0 - 12 + 176, toolsY, 160, 40);
	push('browser', 'tool', AX0 - 12 + 352, toolsY, 160, 40);
	push('mike', 'tool', AX0 - 12 + 88, toolsY + 52, 160, 40);
	push('xmcp', 'tool', AX0 - 12 + 264, toolsY + 52, 160, 40);

	const chipsByKey = new Map(chips.map((c) => [c.key, c] as const));
	const upkeep = chipsByKey.get('upkeep')!;
	const footY = Math.max(toolsY + 52 + 40 + 18, upkeep.y + upkeep.h + 12);
	const stageH = Math.max(footY + 20, 640);
	const stageW = 1372;

	const strip = {
		x: STRIP_X,
		w: STRIP_W,
		spineReq: SPINE_REQ,
		spineRes: SPINE_RES,
		spineReqR: SPINE_REQ_R,
		spineResR: SPINE_RES_R,
		spineEvt: SPINE_EVT,
		floorReq: FLOOR_REQ,
		floorRes: FLOOR_RES,
		evtY: EVT_Y
	};
	const grid = {
		cols: COLS,
		rows: ROWS,
		pitchX: PITCH_X,
		pitchY: PITCH_Y,
		x0: AX0,
		y0: AY0,
		columnX: A_X,
		columnW: A_W
	};

	const defaultCalls: Array<[string, string]> = collapsed
		? [
				['agents', 'shell'],
				['agents', 'files'],
				['agents', 'browser'],
				['agents', 'mike'],
				['agents', 'xmcp']
			]
		: [
				['w1', 'shell'],
				['w3', 'shell'],
				['w8', 'shell'],
				['w10', 'mike'],
				['w14', 'files'],
				['w16', 'files'],
				['w19', 'xmcp'],
				['w20', 'browser']
			];
	const events: Array<[string, string]> = collapsed
		? [
				['inject', 'coord'],
				['filters', 'agents']
			]
		: [
				['inject', 'coord'],
				['filters', 'w10']
			];
	const mikeOut: Array<[string, string]> = [
		['judge', 'openrouter'],
		['verifier', 'openrouter'],
		['embed', 'ollama']
	];

	const input: BoardInput = {
		stageW,
		stageH,
		strip,
		outsideX: OUT_X,
		chips,
		grid,
		collapsed,
		selected: collapsed ? 'agents' : 'w10',
		calls: callsOverride ?? defaultCalls,
		events,
		mikeOut,
		bandY0,
		footY
	};

	return { input, chipsByKey };
}

function allRoutes(routes: BoardRoutes): Array<{ key: string; points: readonly Point[] }> {
	return [...Object.values(routes.top), ...Object.values(routes.bottom)];
}

function assertNoIntrusion(input: BoardInput, routes: BoardRoutes): void {
	for (const route of allRoutes(routes)) {
		for (let i = 0; i < route.points.length - 1; i++) {
			const seg: [Point, Point] = [route.points[i]!, route.points[i + 1]!];
			for (const c of input.chips) {
				expect(entersRect(seg, c), `route "${route.key}" segment ${i} enters chip "${c.key}"`).toBe(
					false
				);
			}
		}
	}
}

function assertInBounds(input: BoardInput, routes: BoardRoutes): void {
	for (const route of allRoutes(routes)) {
		for (const [x, y] of route.points) {
			expect(x).toBeGreaterThanOrEqual(0);
			expect(x).toBeLessThanOrEqual(input.stageW);
			expect(y).toBeGreaterThanOrEqual(0);
			expect(y).toBeLessThanOrEqual(input.stageH);
		}
	}
}

describe.each([
	['expanded', false],
	['collapsed', true]
])('routeBoard — %s fixture', (_label, collapsed) => {
	const { input } = buildFixture(collapsed as boolean);
	const routes = routeBoard(input);

	it('has no same-layer crossings among top routes', () => {
		expect(routes.crossings).toEqual([]);
	});

	it('needs no hop: the shipped call set routes crossing-free by the lane rules', () => {
		for (const [key, hops] of Object.entries(routes.hops)) expect(hops, key).toEqual([]);
	});

	it('never routes a segment into a chip it does not belong to', () => {
		assertNoIntrusion(input, routes);
	});

	it('keeps every point within the stage bounds', () => {
		assertInBounds(input, routes);
	});

	it('keeps parallel same-direction traces at or beyond pitch clearance', () => {
		const topRoutes = Object.values(routes.top);
		expect(tooClose(topRoutes, PITCH)).toEqual([]);
		const bottomRoutes = Object.values(routes.bottom);
		expect(tooClose(bottomRoutes, PITCH)).toEqual([]);
	});
});

describe('routeBoard — expanded fixture channel offsets', () => {
	const { input, chipsByKey } = buildFixture(false);
	const routes = routeBoard(input);

	it('jogs model traffic into the row channel at -4/-7/-10 by column', () => {
		const expectedByCol: Record<number, number> = { 0: -10, 1: -7, 2: -4 };
		for (const key of ['w0', 'w1', 'w2']) {
			const chip = chipsByKey.get(key)!;
			const col = parseInt(key.slice(1), 10) % COLS;
			const route = routes.top[key]!;
			expect(route.points).toHaveLength(6);
			const chY = route.points[2]![1];
			expect(chY).toBeCloseTo(chip.y + expectedByCol[col]!, 6);
		}
	});

	it('jogs a tool call into the band at +3/+6 by order within its row', () => {
		const w19 = chipsByKey.get('w19')!;
		const w20 = chipsByKey.get('w20')!;
		const r19 = routes.top['tool:w19:xmcp']!;
		const r20 = routes.top['tool:w20:browser']!;
		expect(r19.points[1]![1]).toBeCloseTo(w19.y + w19.h + 3, 6);
		expect(r20.points[1]![1]).toBeCloseTo(w20.y + w20.h + 6, 6);
	});

	it('offsets dispatch into the row channel at -13 when the target is off column 0', () => {
		const w10 = chipsByKey.get('w10')!;
		const dispatch = routes.bottom['dispatch']!;
		expect(dispatch.points).toHaveLength(6);
		expect(dispatch.points[2]![1]).toBeCloseTo(w10.y - 13, 6);
	});

	it('offsets a Mike event into the row channel at -16 for an agent below row 0', () => {
		const w10 = chipsByKey.get('w10')!;
		const event = routes.bottom['event:filters:w10']!;
		const chYs = event.points.map(([, y]) => y);
		expect(chYs).toContain(w10.y - 16);
	});
});

describe('routeBoard — hops on an out-of-order tool-call set', () => {
	it('records a hop where one call route crosses another, and tracePath draws an arc there', () => {
		const { input } = buildFixture(false, [
			['w8', 'shell'],
			['w1', 'xmcp'],
			['w5', 'mike']
		]);
		const routes = routeBoard(input);

		expect(routes.hops['tool:w8:shell']).toEqual(
			expect.arrayContaining([expect.arrayContaining([360, 646])])
		);

		const route = routes.top['tool:w8:shell']!;
		const d = tracePath(route.points, 6, routes.hops['tool:w8:shell']!);
		expect(d).toContain('A 4 4 0 0');
	});
});
