import { chipPads, crossings as computeCrossings } from './geometry';
import { PITCH, type ChipRect, type PadSide, type Point, type Route } from './types';

export interface StripLayout {
	readonly x: number;
	readonly w: number;
	readonly spineReq: number;
	readonly spineRes: number;
	readonly spineReqR: number;
	readonly spineResR: number;
	readonly spineEvt: number;
	readonly floorReq: number;
	readonly floorRes: number;
	readonly evtY: number;
}

export interface GridLayout {
	readonly cols: number;
	readonly rows: number;
	readonly pitchX: number;
	readonly pitchY: number;
	readonly x0: number;
	readonly y0: number;
	readonly columnX: number;
	readonly columnW: number;
}

export interface BoardInput {
	readonly stageW: number;
	readonly stageH: number;
	readonly strip: StripLayout;
	readonly outsideX: number;
	readonly chips: readonly ChipRect[];
	readonly grid: GridLayout;
	readonly collapsed: boolean;
	readonly selected?: string;
	readonly calls: ReadonlyArray<readonly [agentKey: string, toolKey: string]>;
	readonly events: ReadonlyArray<readonly [chipKey: string, targetKey: string]>;
	readonly mikeOut: ReadonlyArray<readonly [chipKey: string, providerKey: string]>;
	readonly bandY0: number;
	readonly footY: number;
}

export interface PadAnchor {
	readonly key: string;
	readonly x: number;
	readonly y: number;
	readonly layer: 'top' | 'bottom';
	readonly n: number;
	readonly vertical: boolean;
	readonly owner: string;
	readonly side: PadSide;
}

export interface BoardRoutes {
	readonly top: Record<string, Route>;
	readonly bottom: Record<string, Route>;
	readonly hops: Record<string, Point[]>;
	readonly padAnchors: PadAnchor[];
	readonly crossings: Array<[string, string, Point]>;
}

const REQUEST_PATH_KEYS = ['r_capture', 'filters', 'composer', 'inject', 'compress'] as const;
const RESPONSE_PATH_KEYS = ['s_capture', 'intercept', 'analyzers', 'extract', 'persist'] as const;
const BACKGROUND_KEYS = ['judge', 'verifier', 'embed', 'mailbox', 'upkeep'] as const;
const TOOL_KEYS = ['shell', 'files', 'browser', 'mike', 'xmcp'] as const;

const pt = (x: number, y: number): Point => [x, y];
const VERTICAL_EDGE_SIDES = new Set<PadSide>([
	'right',
	'righthi',
	'rightlo',
	'left',
	'lefthi',
	'leftlo'
]);

function agentIndex(key: string): number {
	return parseInt(key.slice(1), 10);
}

export function routeBoard(input: BoardInput): BoardRoutes {
	const byKey = new Map(input.chips.map((c) => [c.key, c] as const));
	const chip = (key: string): ChipRect => {
		const c = byKey.get(key);
		if (!c) throw new Error(`routeBoard: no chip for key "${key}"`);
		return c;
	};
	const padOf = (key: string, side: PadSide): Point => chipPads(chip(key))[side];

	const { strip, grid } = input;
	const modelChip = chip('model');
	const portY = chipPads(modelChip).left[1];
	const portIn = pt(strip.x, portY);

	const agentKeys = input.collapsed
		? []
		: input.chips
				.filter((c) => /^w\d+$/.test(c.key))
				.map((c) => c.key)
				.sort((a, b) => agentIndex(a) - agentIndex(b));

	const top: Record<string, Route> = {};
	const bottom: Record<string, Route> = {};
	const padAnchors: PadAnchor[] = [];
	const hops: Record<string, Point[]> = {};

	const pushPad = (
		tag: string,
		owner: string,
		side: PadSide,
		p: Point,
		layer: 'top' | 'bottom',
		n = 1
	): void => {
		padAnchors.push({
			key: `${tag}:${owner}:${side}`,
			x: p[0],
			y: p[1],
			layer,
			n,
			vertical: VERTICAL_EDGE_SIDES.has(side),
			owner,
			side
		});
	};

	interface ModelNode {
		key: string;
		start: Point;
		jogX: number;
		chY: number;
	}
	const modelNodes: ModelNode[] = [];
	for (const key of agentKeys) {
		const c = chip(key);
		const i = agentIndex(key);
		const col = i % grid.cols;
		modelNodes.push({
			key,
			start: chipPads(c).right,
			jogX: c.x + c.w + 4 + (grid.cols - 1 - col) * PITCH,
			chY: c.y - 4 - (grid.cols - 1 - col) * PITCH
		});
	}
	if (input.collapsed) {
		const g = chip('agents');
		const gRight = chipPads(g).right;
		modelNodes.push({ key: 'agents', start: gRight, jogX: g.x + g.w + 4, chY: gRight[1] });
	}
	(['coord', 'peer', 'side'] as const).forEach((key, j) => {
		const c = byKey.get(key);
		if (!c) return;
		modelNodes.push({
			key,
			start: chipPads(c).right,
			jogX: 244 + j * PITCH,
			chY: 80 + j * PITCH
		});
	});
	const nFan = modelNodes.length;
	const fanX0 = strip.x - 6 - (nFan - 1) * PITCH;
	const sortedModelNodes = modelNodes.slice().sort((a, b) => a.chY - b.chY);
	sortedModelNodes.forEach((node, rank) => {
		const tx = fanX0 + rank * PITCH;
		const route: Point[] =
			Math.abs(node.chY - node.start[1]) < 0.01
				? [node.start, pt(tx, node.chY), pt(tx, portY), portIn]
				: [
						node.start,
						pt(node.jogX, node.start[1]),
						pt(node.jogX, node.chY),
						pt(tx, node.chY),
						pt(tx, portY),
						portIn
					];
		top[node.key] = { key: node.key, points: route };
		pushPad('right', node.key, 'right', node.start, 'top');
	});

	const reqChannel: Point[] = [
		portIn,
		pt(strip.spineReq, portY),
		pt(strip.spineReq, strip.floorReq),
		pt(strip.spineReqR, strip.floorReq),
		pt(strip.spineReqR, portY),
		pt(strip.x + strip.w, portY),
		chipPads(modelChip).left
	];
	const resChannel: Point[] = [
		pt(modelChip.x, portY - 3),
		pt(strip.x + strip.w, portY - 3),
		pt(strip.spineResR, portY - 3),
		pt(strip.spineResR, strip.floorRes),
		pt(strip.spineRes, strip.floorRes),
		pt(strip.spineRes, portY - 3),
		pt(strip.x, portY - 3)
	];
	top['req_channel'] = { key: 'req_channel', points: reqChannel };
	top['res_channel'] = { key: 'res_channel', points: resChannel };

	if (byKey.has('upkeep')) {
		const upkeep = chip('upkeep');
		top['bg_bus'] = {
			key: 'bg_bus',
			points: [pt(strip.spineReq, strip.floorReq), pt(strip.spineReq, upkeep.y + 8)]
		};
	}
	const tapL = (key: string): Point[] => {
		const c = chip(key);
		return [pt(strip.spineReq, c.y + 8), pt(c.x, c.y + 8)];
	};
	const tapR = (key: string): Point[] => {
		const c = chip(key);
		return [pt(strip.spineResR, c.y + 8), pt(c.x + c.w, c.y + 8)];
	};
	for (const k of [...REQUEST_PATH_KEYS, ...BACKGROUND_KEYS]) {
		if (!byKey.has(k)) continue;
		top[`tap:${k}`] = { key: `tap:${k}`, points: tapL(k) };
		pushPad('tap', k, 'lefthi', padOf(k, 'lefthi'), 'top');
	}
	for (const k of RESPONSE_PATH_KEYS) {
		if (!byKey.has(k)) continue;
		top[`tap:${k}`] = { key: `tap:${k}`, points: tapR(k) };
		pushPad('tap', k, 'righthi', padOf(k, 'righthi'), 'top');
	}
	for (const k of [...REQUEST_PATH_KEYS, ...RESPONSE_PATH_KEYS, ...BACKGROUND_KEYS]) {
		if (!byKey.has(k)) continue;
		pushPad('via', k, 'leftlo', padOf(k, 'leftlo'), 'bottom');
	}

	interface Stub {
		agentKey: string;
		toolKey: string;
		start: Point;
		sx: number;
		jogY: number | null;
		laneY: number;
		i: number;
	}
	const gutX0 = grid.columnX + 4;
	let stubs: Stub[];
	const arrivals: Record<string, number[]> = {};
	if (input.collapsed) {
		const g = chip('agents');
		const padx = chipPads(g).bottomc[0];
		const order = input.calls
			.map((_, i) => i)
			.sort((a, b) => padOf(input.calls[a]![1], 'topc')[0] - padOf(input.calls[b]![1], 'topc')[0]);
		const left = order.filter((i) => padOf(input.calls[i]![1], 'topc')[0] < padx);
		const right = order.filter((i) => padOf(input.calls[i]![1], 'topc')[0] >= padx);
		const lane: Record<number, number> = {};
		left.forEach((i, r) => (lane[i] = r));
		right
			.slice()
			.reverse()
			.forEach((i, r) => (lane[i] = left.length + r));
		const stubX: Record<number, number> = {};
		order.forEach((i, j) => (stubX[i] = padx + PITCH * (j - (order.length - 1) / 2)));
		input.calls.forEach(([, toolKey], i) => {
			(arrivals[toolKey] ??= []).push(i);
		});
		stubs = input.calls.map(([agentKey, toolKey], i) => ({
			agentKey,
			toolKey,
			start: pt(stubX[i]!, g.y + g.h),
			sx: stubX[i]!,
			jogY: null,
			laneY: input.bandY0 + lane[i]! * PITCH,
			i
		}));
	} else {
		const order = input.calls
			.map((_, i) => i)
			.sort((a, b) => agentIndex(input.calls[a]![0]) - agentIndex(input.calls[b]![0]));
		const slot: Record<number, number> = {};
		order.forEach((i, j) => (slot[i] = j));
		const goingLeft = order.filter(
			(i) => padOf(input.calls[i]![1], 'topc')[0] < gutX0 + slot[i]! * PITCH
		);
		const goingLeftSet = new Set(goingLeft);
		const goingRight = order.filter((i) => !goingLeftSet.has(i));
		const lane: Record<number, number> = {};
		goingLeft.forEach((i, r) => (lane[i] = r));
		goingRight
			.slice()
			.reverse()
			.forEach((i, r) => (lane[i] = goingLeft.length + r));
		const rowsSeen: Record<number, number> = {};
		stubs = [];
		for (const i of order) {
			const [agentKey, toolKey] = input.calls[i]!;
			const c = chip(agentKey);
			const row = Math.floor(agentIndex(agentKey) / grid.cols);
			const k = rowsSeen[row] ?? 0;
			rowsSeen[row] = k + 1;
			stubs.push({
				agentKey,
				toolKey,
				start: chipPads(c).bottomc,
				sx: gutX0 + slot[i]! * PITCH,
				jogY: c.y + c.h + 3 + k * PITCH,
				laneY: input.bandY0 + lane[i]! * PITCH,
				i
			});
			(arrivals[toolKey] ??= []).push(i);
		}
	}
	const laneYOf: Record<number, number> = {};
	const sxOf: Record<number, number> = {};
	for (const s of stubs) {
		laneYOf[s.i] = s.laneY;
		sxOf[s.i] = s.sx;
	}
	const turnFor: Record<number, number> = {};
	for (const [toolKey, list] of Object.entries(arrivals)) {
		const padx = padOf(toolKey, 'topc')[0];
		const fromLeft = list.filter((i) => sxOf[i]! < padx).sort((a, b) => laneYOf[b]! - laneYOf[a]!);
		const fromRight = list
			.filter((i) => sxOf[i]! >= padx)
			.sort((a, b) => laneYOf[a]! - laneYOf[b]!);
		const seq = [...fromLeft, ...fromRight];
		seq.forEach((i, j) => {
			turnFor[i] = padx + PITCH * (j - (seq.length - 1) / 2);
		});
	}
	const toolRoutes: Array<{ agentKey: string; toolKey: string; points: Point[] }> = [];
	for (const s of stubs) {
		const tk_top = padOf(s.toolKey, 'topc')[1];
		const tx = turnFor[s.i]!;
		const points: Point[] =
			s.jogY === null
				? [s.start, pt(s.sx, s.laneY), pt(tx, s.laneY), pt(tx, tk_top)]
				: [
						s.start,
						pt(s.start[0], s.jogY),
						pt(s.sx, s.jogY),
						pt(s.sx, s.laneY),
						pt(tx, s.laneY),
						pt(tx, tk_top)
					];
		toolRoutes.push({ agentKey: s.agentKey, toolKey: s.toolKey, points });
	}
	for (const r of toolRoutes) {
		const key = `tool:${r.agentKey}:${r.toolKey}`;
		top[key] = { key, points: r.points };
	}
	if (input.collapsed && byKey.has('agents')) {
		const g = chip('agents');
		pushPad('calls', 'agents', 'bottomc', chipPads(g).bottomc, 'top', input.calls.length);
	}
	for (const agentKey of agentKeys) {
		const n = input.calls.filter(([a]) => a === agentKey).length;
		pushPad('calls', agentKey, 'bottomc', padOf(agentKey, 'bottomc'), 'top', Math.max(1, n));
	}
	for (const tk of TOOL_KEYS) {
		if (!byKey.has(tk)) continue;
		pushPad(
			'arrivals',
			tk,
			'topc',
			padOf(tk, 'topc'),
			'top',
			Math.max(1, arrivals[tk]?.length ?? 0)
		);
	}

	for (let idx = 0; idx < toolRoutes.length; idx++) {
		const r = toolRoutes[idx]!;
		const key = `tool:${r.agentKey}:${r.toolKey}`;
		const pts = r.points;
		const found: Point[] = [];
		for (let j = 0; j < toolRoutes.length; j++) {
			if (j === idx) continue;
			const other = toolRoutes[j]!.points;
			for (let s = 0; s < pts.length - 1; s++) {
				const [x0, y0] = pts[s]!;
				const [x1, y1] = pts[s + 1]!;
				if (Math.abs(x0 - x1) > 0.01) continue;
				for (let t = 0; t < other.length - 1; t++) {
					const [ox0, oy0] = other[t]!;
					const [ox1, oy1] = other[t + 1]!;
					const oHoriz = Math.abs(oy0 - oy1) < 0.01;
					if (!oHoriz) continue;
					const [xlo, xhi] = [Math.min(ox0, ox1), Math.max(ox0, ox1)];
					const [ylo, yhi] = [Math.min(y0, y1), Math.max(y0, y1)];
					if (xlo + 0.5 < x0 && x0 < xhi - 0.5 && ylo + 0.5 < oy0 && oy0 < yhi - 0.5) {
						found.push(pt(x0, oy0));
					}
				}
			}
		}
		hops[key] = found;
	}

	const browser = chip('browser');
	const xmcp = chip('xmcp');
	const internet = chip('internet');
	const inetY = chipPads(internet).left[1];
	const browserR: Point[] = [
		chipPads(browser).bottomr,
		pt(chipPads(browser).bottomr[0], input.footY),
		pt(strip.x, input.footY)
	];
	const xmcpR: Point[] = [
		chipPads(xmcp).bottomr,
		pt(chipPads(xmcp).bottomr[0], input.footY),
		pt(strip.x, input.footY)
	];
	const inetTrunk: Point[] = [
		pt(strip.x, input.footY),
		pt(input.outsideX - 2, input.footY),
		pt(input.outsideX - 2, inetY),
		chipPads(internet).left
	];
	top['browser'] = { key: 'browser', points: browserR };
	top['xmcp'] = { key: 'xmcp', points: xmcpR };
	top['inet_trunk'] = { key: 'inet_trunk', points: inetTrunk };
	pushPad('bottomr', 'browser', 'bottomr', chipPads(browser).bottomr, 'top');
	pushPad('bottomr', 'xmcp', 'bottomr', chipPads(xmcp).bottomr, 'top');
	pushPad('port', 'strip', 'left', pt(strip.x, portY - 1.5), 'top');
	pushPad('port', 'strip', 'right', pt(strip.x + strip.w, portY - 1.5), 'top');
	pushPad('port', 'model', 'left', pt(modelChip.x, portY - 1.5), 'top');
	pushPad('foot', 'strip', 'left', pt(strip.x, input.footY), 'top');
	pushPad('foot', 'strip', 'right', pt(strip.x + strip.w, input.footY), 'top');
	pushPad('foot', 'internet', 'left', chipPads(internet).left, 'top');

	input.mikeOut.forEach(([chipKey, providerKey], j) => {
		const c = chip(chipKey);
		const pv = chip(providerKey);
		const cRight = chipPads(c).right;
		const pvLeft = chipPads(pv).left;
		const jog = strip.x + strip.w + 6 + j * 6;
		const route: Point[] = [cRight, pt(jog, cRight[1]), pt(jog, pvLeft[1]), pvLeft];
		const key = `mike:${chipKey}`;
		top[key] = { key, points: route };
		pushPad('mikeout', chipKey, 'right', cRight, 'top');
		pushPad('mikeout', providerKey, 'left', pvLeft, 'top');
	});

	const coord = chip('coord');
	const term = chip('term');
	const peer = byKey.get('peer');
	const termLefthi = chipPads(term).lefthi;
	const coordLeft = chipPads(coord).left;
	const termR: Point[] = [termLefthi, pt(48, termLefthi[1]), pt(48, coordLeft[1]), coordLeft];
	bottom['term'] = { key: 'term', points: termR };
	pushPad('message', 'term', 'lefthi', termLefthi, 'bottom');
	pushPad('message', 'coord', 'left', coordLeft, 'bottom');

	const targetKey = input.collapsed ? 'agents' : (input.selected ?? '');
	if (byKey.has(targetKey)) {
		const target = chip(targetKey);
		const coordRighthi = chipPads(coord).righthi;
		const targetLeft = chipPads(target).left;
		const dispatchR: Point[] =
			input.collapsed || agentIndex(targetKey) % grid.cols === 0
				? [coordRighthi, pt(260, coordRighthi[1]), pt(260, targetLeft[1]), targetLeft]
				: (() => {
						const chY = target.y - 13;
						const gx = target.x - 6;
						return [
							coordRighthi,
							pt(260, coordRighthi[1]),
							pt(260, chY),
							pt(gx, chY),
							pt(gx, targetLeft[1]),
							targetLeft
						];
					})();
		bottom['dispatch'] = { key: 'dispatch', points: dispatchR };
		pushPad('dispatch', 'coord', 'righthi', coordRighthi, 'bottom');
		pushPad('dispatch', targetKey, 'left', targetLeft, 'bottom');
		for (const k of agentKeys) {
			if (k === targetKey) continue;
			pushPad('idle', k, 'left', padOf(k, 'left'), 'bottom');
		}
	}

	if (peer) {
		const peerTop = chipPads(peer).top;
		const coordBottomr = chipPads(coord).bottomr;
		const mailR: Point[] = [peerTop, coordBottomr];
		bottom['mail'] = { key: 'mail', points: mailR };
		pushPad('mail', 'peer', 'top', peerTop, 'bottom');
		pushPad('mail', 'coord', 'bottomr', coordBottomr, 'bottom');
	}

	const sessionTopKeys = ['coord', 'side', 'peer', ...(input.collapsed ? ['agents'] : agentKeys)];
	for (const k of sessionTopKeys) {
		if (!byKey.has(k)) continue;
		pushPad('event', k, 'top', padOf(k, 'top'), 'bottom');
	}

	const mikeEvent = (chipKey: string, targetKeyEv: string): Point[] => {
		const c = chip(chipKey);
		const t = chip(targetKeyEv);
		const src = pt(c.x, c.y + 20);
		const topPad = chipPads(t).top;
		if (/^w\d+$/.test(targetKeyEv) && Math.floor(agentIndex(targetKeyEv) / grid.cols) > 0) {
			const chY = t.y - 16;
			return [
				src,
				pt(strip.spineEvt, src[1]),
				pt(strip.spineEvt, strip.evtY),
				pt(grid.columnX - 1, strip.evtY),
				pt(grid.columnX - 1, chY),
				pt(topPad[0], chY),
				topPad
			];
		}
		return [
			src,
			pt(strip.spineEvt, src[1]),
			pt(strip.spineEvt, strip.evtY),
			pt(topPad[0], strip.evtY),
			topPad
		];
	};
	for (const [chipKey, targetKeyEv] of input.events) {
		const key = `event:${chipKey}:${targetKeyEv}`;
		bottom[key] = { key, points: mikeEvent(chipKey, targetKeyEv) };
	}

	const isHopped = (routeKey: string, p: Point): boolean =>
		(hops[routeKey] ?? []).some(
			([hx, hy]) => Math.abs(hx - p[0]) < 0.01 && Math.abs(hy - p[1]) < 0.01
		);
	const unresolvedCrossings = computeCrossings(Object.values(top)).filter(
		([a, b, p]) => !isHopped(a, p) && !isHopped(b, p)
	);

	return {
		top,
		bottom,
		hops,
		padAnchors,
		crossings: unresolvedCrossings
	};
}
