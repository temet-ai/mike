export interface LayoutNode {
	id: string;
}

export interface LayoutEdge {
	from: string;
	to: string;
}

export interface NodePosition {
	x: number;
	y: number;
}

export interface CanvasLayout {
	positions: Map<string, NodePosition>;
	levels: Map<string, number>;
	parent: Map<string, string>;
	width: number;
	height: number;
	focusNodeId: string | null;
}

export interface ComponentInfo {
	ids: string[];
	hubId: string;
}

export const NODE_WIDTH = 220;
export const NODE_HEIGHT = 128;
const GAP_X = 56;
const GAP_Y = 104;
export const ROW_PITCH = NODE_HEIGHT + GAP_Y;
const PAD = 72;
const ISOLATED_COLS = 6;
const LANE_GAP = 7;
const LATERAL_DETOUR = 18;
const MARGIN_GAP = 40;

function buildAdjacency(nodes: LayoutNode[], edges: LayoutEdge[]): Map<string, Set<string>> {
	const idSet = new Set(nodes.map((n) => n.id));
	const adjacency = new Map<string, Set<string>>(nodes.map((n) => [n.id, new Set<string>()]));
	for (const e of edges) {
		if (!idSet.has(e.from) || !idSet.has(e.to) || e.from === e.to) continue;
		adjacency.get(e.from)?.add(e.to);
		adjacency.get(e.to)?.add(e.from);
	}
	return adjacency;
}

function connectedComponents(nodes: LayoutNode[], adjacency: Map<string, Set<string>>): string[][] {
	const seen = new Set<string>();
	const components: string[][] = [];
	for (const n of nodes) {
		if (seen.has(n.id)) continue;
		const component: string[] = [];
		const queue = [n.id];
		seen.add(n.id);
		let head = 0;
		while (head < queue.length) {
			const id = queue[head++];
			if (id === undefined) continue;
			component.push(id);
			for (const next of adjacency.get(id) ?? []) {
				if (seen.has(next)) continue;
				seen.add(next);
				queue.push(next);
			}
		}
		components.push(component);
	}
	return components;
}

function levelsWithinComponent(
	ids: string[],
	adjacency: Map<string, Set<string>>
): { level: Map<string, number>; centerId: string; parent: Map<string, string> } {
	const byDegreeDesc = [...ids].sort((a, b) => {
		const d = (adjacency.get(b)?.size ?? 0) - (adjacency.get(a)?.size ?? 0);
		return d !== 0 ? d : a.localeCompare(b);
	});
	const level = new Map<string, number>();
	const parent = new Map<string, string>();
	const center = byDegreeDesc[0] ?? ids[0] ?? '';
	level.set(center, 0);
	const queue = [center];
	let head = 0;
	while (head < queue.length) {
		const id = queue[head++];
		if (id === undefined) continue;
		const here = level.get(id) ?? 0;
		for (const next of adjacency.get(id) ?? []) {
			if (level.has(next)) continue;
			level.set(next, here + 1);
			parent.set(next, id);
			queue.push(next);
		}
	}
	return { level, centerId: center, parent };
}

export function computeComponents(nodes: LayoutNode[], edges: LayoutEdge[]): ComponentInfo[] {
	const adjacency = buildAdjacency(nodes, edges);
	const comps = connectedComponents(nodes, adjacency).filter((ids) => ids.length >= 2);
	const infos = comps.map((ids) => {
		const { centerId } = levelsWithinComponent(ids, adjacency);
		return { ids, hubId: centerId };
	});
	infos.sort((a, b) => {
		const d = b.ids.length - a.ids.length;
		if (d !== 0) return d;
		const aMin = a.ids.reduce((m, id) => (id < m ? id : m), a.ids[0] ?? '');
		const bMin = b.ids.reduce((m, id) => (id < m ? id : m), b.ids[0] ?? '');
		return aMin.localeCompare(bMin);
	});
	return infos;
}

export function buildChildrenMap(parent: Map<string, string>): Map<string, string[]> {
	const children = new Map<string, string[]>();
	for (const [child, p] of parent) {
		const arr = children.get(p) ?? [];
		arr.push(child);
		children.set(p, arr);
	}
	return children;
}

export function subtreeIds(childrenMap: Map<string, string[]>, rootId: string): string[] {
	const out: string[] = [];
	const queue = [...(childrenMap.get(rootId) ?? [])];
	while (queue.length > 0) {
		const id = queue.shift();
		if (id === undefined) continue;
		out.push(id);
		queue.push(...(childrenMap.get(id) ?? []));
	}
	return out;
}

interface Block {
	positions: Map<string, NodePosition>;
	levels: Map<string, number>;
	parent: Map<string, string>;
	width: number;
	height: number;
	centerId: string;
}

function layoutHubComponent(ids: string[], adjacency: Map<string, Set<string>>): Block {
	const { level, centerId, parent } = levelsWithinComponent(ids, adjacency);
	const byLevel = new Map<number, string[]>();
	for (const id of ids) {
		const lv = level.get(id) ?? 0;
		const bucket = byLevel.get(lv) ?? [];
		bucket.push(id);
		byLevel.set(lv, bucket);
	}
	for (const bucket of byLevel.values()) bucket.sort();

	const sortedLevels = Array.from(byLevel.keys()).sort((a, b) => a - b);
	const maxPerLevel = Math.max(...Array.from(byLevel.values(), (b) => b.length));
	const width = maxPerLevel * NODE_WIDTH + (maxPerLevel - 1) * GAP_X;
	const height = sortedLevels.length * NODE_HEIGHT + (sortedLevels.length - 1) * GAP_Y;

	const positions = new Map<string, NodePosition>();
	sortedLevels.forEach((lv, rowIndex) => {
		const rowIds = byLevel.get(lv) ?? [];
		const rowWidth = rowIds.length * NODE_WIDTH + (rowIds.length - 1) * GAP_X;
		const startX = (width - rowWidth) / 2 + NODE_WIDTH / 2;
		const y = NODE_HEIGHT / 2 + rowIndex * (NODE_HEIGHT + GAP_Y);
		rowIds.forEach((id, i) => {
			positions.set(id, { x: startX + i * (NODE_WIDTH + GAP_X), y });
		});
	});

	return { positions, levels: level, parent, width, height, centerId };
}

function layoutIsolatedGrid(ids: string[]): Block {
	const sorted = [...ids].sort();
	const cols = Math.min(ISOLATED_COLS, sorted.length);
	const rows = Math.ceil(sorted.length / cols);
	const width = cols * NODE_WIDTH + (cols - 1) * GAP_X;
	const positions = new Map<string, NodePosition>();
	sorted.forEach((id, i) => {
		const row = Math.floor(i / cols);
		const col = i % cols;
		const rowIds = sorted.slice(row * cols, row * cols + cols);
		const rowWidth = rowIds.length * NODE_WIDTH + (rowIds.length - 1) * GAP_X;
		const startX = (width - rowWidth) / 2 + NODE_WIDTH / 2;
		positions.set(id, {
			x: startX + col * (NODE_WIDTH + GAP_X),
			y: NODE_HEIGHT / 2 + row * (NODE_HEIGHT + GAP_Y)
		});
	});
	return {
		positions,
		levels: new Map(),
		parent: new Map(),
		width,
		height: rows * NODE_HEIGHT + (rows - 1) * GAP_Y,
		centerId: sorted[0] ?? ''
	};
}

export function layeredLayout(nodes: LayoutNode[], edges: LayoutEdge[]): CanvasLayout {
	if (nodes.length === 0) {
		return {
			positions: new Map(),
			levels: new Map(),
			parent: new Map(),
			width: NODE_WIDTH + 2 * PAD,
			height: NODE_HEIGHT + 2 * PAD,
			focusNodeId: null
		};
	}

	const adjacency = buildAdjacency(nodes, edges);
	const components = connectedComponents(nodes, adjacency).map((ids) => ({
		ids,
		isIsolated: ids.length === 1,
		minId: ids.reduce((min, id) => (id < min ? id : min), ids[0] ?? '')
	}));
	components.sort((a, b) => {
		if (a.isIsolated !== b.isIsolated) return a.isIsolated ? 1 : -1;
		const d = b.ids.length - a.ids.length;
		return d !== 0 ? d : a.minId.localeCompare(b.minId);
	});

	const hubBlocks = components
		.filter((c) => !c.isIsolated)
		.map((c) => layoutHubComponent(c.ids, adjacency));
	const isolatedIds = components.filter((c) => c.isIsolated).flatMap((c) => c.ids);

	const blocks: Block[] = [...hubBlocks];
	if (isolatedIds.length > 0) blocks.push(layoutIsolatedGrid(isolatedIds));

	const width = Math.max(...blocks.map((b) => b.width)) + 2 * PAD;
	const positions = new Map<string, NodePosition>();
	const levels = new Map<string, number>();
	const parent = new Map<string, string>();
	let yCursor = PAD;
	blocks.forEach((block, i) => {
		if (i > 0) yCursor += GAP_Y;
		const xOffset = PAD + (width - 2 * PAD - block.width) / 2;
		for (const [id, pos] of block.positions) {
			positions.set(id, { x: pos.x + xOffset, y: pos.y + yCursor });
		}
		for (const [id, lv] of block.levels) levels.set(id, lv);
		for (const [id, p] of block.parent) parent.set(id, p);
		yCursor += block.height;
	});

	const focusNodeId = blocks[0]?.centerId ?? null;

	return { positions, levels, parent, width, height: yCursor + PAD, focusNodeId };
}

function roundCorners(rawPts: [number, number][]): string {
	const pts = rawPts.filter((p, i) => {
		if (i === 0) return true;
		const prev = rawPts[i - 1];
		return prev === undefined || p[0] !== prev[0] || p[1] !== prev[1];
	});
	if (pts.length < 3) {
		const a = pts[0];
		const b = pts[pts.length - 1];
		return a && b ? `M ${a[0]} ${a[1]} L ${b[0]} ${b[1]}` : '';
	}
	const R = 12;
	const first = pts[0];
	if (!first) return '';
	let d = `M ${first[0]} ${first[1]}`;
	for (let i = 1; i < pts.length - 1; i++) {
		const p = pts[i - 1];
		const c = pts[i];
		const n = pts[i + 1];
		if (!p || !c || !n) continue;
		const l1 = Math.hypot(c[0] - p[0], c[1] - p[1]);
		const l2 = Math.hypot(n[0] - c[0], n[1] - c[1]);
		const r = Math.min(R, l1 / 2, l2 / 2);
		const e1: [number, number] = [c[0] - ((c[0] - p[0]) / l1) * r, c[1] - ((c[1] - p[1]) / l1) * r];
		const e2: [number, number] = [c[0] + ((n[0] - c[0]) / l2) * r, c[1] + ((n[1] - c[1]) / l2) * r];
		d += ` L ${e1[0]} ${e1[1]} Q ${c[0]} ${c[1]} ${e2[0]} ${e2[1]}`;
	}
	const last = pts[pts.length - 1];
	if (last) d += ` L ${last[0]} ${last[1]}`;
	return d;
}

export function roundedOrthoPath(sx: number, sy: number, tx: number, ty: number): string {
	const dx = tx - sx;
	const dy = ty - sy;
	const pts: [number, number][] =
		Math.abs(dy) >= Math.abs(dx)
			? [
					[sx, sy],
					[sx, sy + dy / 2],
					[tx, sy + dy / 2],
					[tx, ty]
				]
			: [
					[sx, sy],
					[sx + dx / 2, sy],
					[sx + dx / 2, ty],
					[tx, ty]
				];
	return roundCorners(pts);
}

function corridorEdgePath(sx: number, sy: number, tx: number, ty: number, midY: number): string {
	return roundCorners([
		[sx, sy],
		[sx, midY],
		[tx, midY],
		[tx, ty]
	]);
}

function marginEdgePath(sx: number, sy: number, tx: number, ty: number, marginX: number): string {
	return roundCorners([
		[sx, sy],
		[marginX, sy],
		[marginX, ty],
		[tx, ty]
	]);
}

export interface RoutableEdge {
	id: string;
	from: string;
	to: string;
}

export interface RoutedEdge {
	path: string;
	labelX: number;
	labelY: number;
}

export function routeEdges(
	edges: RoutableEdge[],
	positions: Map<string, NodePosition>,
	levels: Map<string, number>
): Map<string, RoutedEdge> {
	const result = new Map<string, RoutedEdge>();
	const adjacent: RoutableEdge[] = [];
	const lateral: RoutableEdge[] = [];
	const skip: RoutableEdge[] = [];
	for (const e of edges) {
		if (!positions.has(e.from) || !positions.has(e.to)) continue;
		const lf = levels.get(e.from) ?? 0;
		const lt = levels.get(e.to) ?? 0;
		const d = Math.abs(lf - lt);
		if (d === 1) adjacent.push(e);
		else if (d === 0) lateral.push(e);
		else skip.push(e);
	}

	const adjacentGroups = new Map<string, RoutableEdge[]>();
	for (const e of adjacent) {
		const lf = levels.get(e.from) ?? 0;
		const lt = levels.get(e.to) ?? 0;
		const key = lf < lt ? `${lf}:${lt}` : `${lt}:${lf}`;
		const arr = adjacentGroups.get(key) ?? [];
		arr.push(e);
		adjacentGroups.set(key, arr);
	}
	for (const group of adjacentGroups.values()) {
		const withChildX = group.map((e) => {
			const lf = levels.get(e.from) ?? 0;
			const lt = levels.get(e.to) ?? 0;
			const childId = lf > lt ? e.from : e.to;
			return { e, childX: positions.get(childId)?.x ?? 0 };
		});
		withChildX.sort((a, b) => a.childX - b.childX || a.e.id.localeCompare(b.e.id));
		const n = withChildX.length;
		withChildX.forEach(({ e }, i) => {
			const from = positions.get(e.from);
			const to = positions.get(e.to);
			if (!from || !to) return;
			const midY = (from.y + to.y) / 2 + (i - (n - 1) / 2) * LANE_GAP;
			const dir = to.y >= from.y ? 1 : -1;
			const sy1 = from.y + dir * (NODE_HEIGHT / 2);
			const ty1 = to.y - dir * (NODE_HEIGHT / 2);
			result.set(e.id, {
				path: corridorEdgePath(from.x, sy1, to.x, ty1, midY),
				labelX: (from.x + to.x) / 2,
				labelY: midY
			});
		});
	}

	const lateralGroups = new Map<number, RoutableEdge[]>();
	for (const e of lateral) {
		const from = positions.get(e.from);
		if (!from) continue;
		const arr = lateralGroups.get(from.y) ?? [];
		arr.push(e);
		lateralGroups.set(from.y, arr);
	}
	for (const [rowY, group] of lateralGroups) {
		group.sort((a, b) => {
			const ax = Math.min(positions.get(a.from)?.x ?? 0, positions.get(a.to)?.x ?? 0);
			const bx = Math.min(positions.get(b.from)?.x ?? 0, positions.get(b.to)?.x ?? 0);
			return ax - bx || a.id.localeCompare(b.id);
		});
		group.forEach((e, i) => {
			const from = positions.get(e.from);
			const to = positions.get(e.to);
			if (!from || !to) return;
			const detourY = rowY + NODE_HEIGHT / 2 + LATERAL_DETOUR + i * LANE_GAP;
			result.set(e.id, {
				path: corridorEdgePath(
					from.x,
					from.y + NODE_HEIGHT / 2,
					to.x,
					to.y + NODE_HEIGHT / 2,
					detourY
				),
				labelX: (from.x + to.x) / 2,
				labelY: detourY
			});
		});
	}

	if (skip.length > 0) {
		const allX = [...positions.values()].map((p) => p.x);
		const minX = Math.min(...allX) - NODE_WIDTH / 2 - MARGIN_GAP;
		const maxX = Math.max(...allX) + NODE_WIDTH / 2 + MARGIN_GAP;
		skip.forEach((e, i) => {
			const from = positions.get(e.from);
			const to = positions.get(e.to);
			if (!from || !to) return;
			const midX = (from.x + to.x) / 2;
			const goLeft = midX <= (minX + maxX) / 2;
			const marginX = goLeft ? minX - i * LANE_GAP : maxX + i * LANE_GAP;
			const dir = to.y >= from.y ? 1 : -1;
			const sy1 = from.y + dir * (NODE_HEIGHT / 2);
			const ty1 = to.y - dir * (NODE_HEIGHT / 2);
			result.set(e.id, {
				path: marginEdgePath(from.x, sy1, to.x, ty1, marginX),
				labelX: marginX,
				labelY: (sy1 + ty1) / 2
			});
		});
	}

	return result;
}
