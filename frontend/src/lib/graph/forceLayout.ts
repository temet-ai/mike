import {
	forceSimulation,
	forceLink,
	forceManyBody,
	forceX,
	forceY,
	type Force,
	type Simulation,
	type SimulationLinkDatum,
	type SimulationNodeDatum
} from 'd3-force';

export interface LayoutNode {
	id: string;
	width?: number;
	height?: number;
}

export interface LayoutEdge {
	from: string;
	to: string;
	similarity: number;
}

export interface NodePosition {
	id: string;
	x: number;
	y: number;
}

export const DEFAULT_NODE_WIDTH = 200;
export const DEFAULT_NODE_HEIGHT = 64;

const ITERATIONS = 220;
const LINK_STRENGTH = 0.4;
const CHARGE_STRENGTH = -520;
const CHARGE_DISTANCE_MIN = 6;
const GRAVITY_STRENGTH = 0.03;
const MARGIN = 22;
const GAP_MAX = 190;
const GAP_MIN = 20;

const AREA_BREATHING_FACTOR = 3.4;

const COLLISION_SWEEPS = 400;
const COLLISION_GAP = 10;

const GOLDEN_ANGLE = 2.399963229728653;

interface SizedCard {
	width: number;
	height: number;
	radius: number;
}

const DEFAULT_RADIUS = Math.hypot(DEFAULT_NODE_WIDTH, DEFAULT_NODE_HEIGHT) / 2;

function sizeOf(node: LayoutNode): SizedCard {
	const width = node.width ?? DEFAULT_NODE_WIDTH;
	const height = node.height ?? DEFAULT_NODE_HEIGHT;
	return { width, height, radius: Math.hypot(width, height) / 2 };
}

function gapFor(similarity: number): number {
	const clamped = Math.min(1, Math.max(0, similarity));
	return GAP_MAX - clamped * (GAP_MAX - GAP_MIN);
}

function seededRandom(seed: number): () => number {
	let state = (seed >>> 0) ^ 0x9e3779b9;
	return function random(): number {
		state |= 0;
		state = (state + 0x6d2b79f5) | 0;
		let t = Math.imul(state ^ (state >>> 15), 1 | state);
		t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

interface SimNode extends SimulationNodeDatum {
	id: string;
	width: number;
	height: number;
	radius: number;
}

interface SimLink extends SimulationLinkDatum<SimNode> {
	source: string;
	target: string;
	restLength: number;
}

function resolveRectCollisions(nodes: readonly SimNode[]): boolean {
	let any = false;
	for (let i = 0; i < nodes.length; i++) {
		for (let j = i + 1; j < nodes.length; j++) {
			const a = nodes[i]!;
			const b = nodes[j]!;
			const dx = (b.x ?? 0) - (a.x ?? 0);
			const dy = (b.y ?? 0) - (a.y ?? 0);
			const overlapX = a.width / 2 + b.width / 2 + COLLISION_GAP - Math.abs(dx);
			const overlapY = a.height / 2 + b.height / 2 + COLLISION_GAP - Math.abs(dy);
			if (overlapX <= 0 || overlapY <= 0) continue;
			any = true;

			let pushX = 0;
			let pushY = 0;
			if (overlapX < overlapY) {
				const dir = dx === 0 ? (a.id < b.id ? -1 : 1) : Math.sign(dx);
				pushX = dir * overlapX;
			} else {
				const dir = dy === 0 ? (a.id < b.id ? -1 : 1) : Math.sign(dy);
				pushY = dir * overlapY;
			}

			const aPinned = a.fx != null && a.fy != null;
			const bPinned = b.fx != null && b.fy != null;
			if (aPinned && bPinned) {
			} else if (aPinned) {
				b.x = (b.x ?? 0) + pushX;
				b.y = (b.y ?? 0) + pushY;
			} else if (bPinned) {
				a.x = (a.x ?? 0) - pushX;
				a.y = (a.y ?? 0) - pushY;
			} else {
				a.x = (a.x ?? 0) - pushX / 2;
				a.y = (a.y ?? 0) - pushY / 2;
				b.x = (b.x ?? 0) + pushX / 2;
				b.y = (b.y ?? 0) + pushY / 2;
			}
		}
	}
	return any;
}

function forceRectCollide(): Force<SimNode, undefined> {
	let nodes: SimNode[] = [];
	const force = (() => {
		resolveRectCollisions(nodes);
	}) as Force<SimNode, undefined>;
	force.initialize = (n: SimNode[]) => {
		nodes = n;
	};
	return force;
}

interface BuiltLayout {
	simulation: Simulation<SimNode, SimLink>;
	simNodes: SimNode[];
	anchorId: string | null;
}

function buildSimulation(
	nodes: readonly LayoutNode[],
	edges: readonly LayoutEdge[],
	width: number,
	height: number,
	centreId: string | null | undefined,
	seed: number
): BuiltLayout {
	const sized = nodes.map((n) => ({ id: n.id, ...sizeOf(n) }));
	const idSet = new Set(sized.map((s) => s.id));
	const validEdges = edges.filter((e) => idSet.has(e.from) && idSet.has(e.to) && e.from !== e.to);
	const anchorId = centreId != null && idSet.has(centreId) ? centreId : null;

	const totalArea = sized.reduce((sum, s) => sum + s.width * s.height, 0);
	const targetSide = Math.sqrt(totalArea * AREA_BREATHING_FACTOR);
	const spread = Math.max(width, height, targetSide);
	const cx = width / 2;
	const cy = height / 2;

	const ringRadius = Math.max(1, spread / 2 - MARGIN) * 0.55;
	const angleOffset = seed * GOLDEN_ANGLE;

	const simNodes: SimNode[] = sized.map((s, i) => {
		if (s.id === anchorId) {
			return {
				id: s.id,
				width: s.width,
				height: s.height,
				radius: s.radius,
				x: cx,
				y: cy,
				fx: cx,
				fy: cy
			};
		}
		const angle = (2 * Math.PI * i) / nodes.length + angleOffset;
		return {
			id: s.id,
			width: s.width,
			height: s.height,
			radius: s.radius,
			x: cx + Math.cos(angle) * ringRadius,
			y: cy + Math.sin(angle) * ringRadius
		};
	});

	const radiusById = new Map(simNodes.map((n) => [n.id, n.radius]));
	const simLinks: SimLink[] = validEdges.map((e) => ({
		source: e.from,
		target: e.to,
		restLength: (radiusById.get(e.from) ?? 0) + (radiusById.get(e.to) ?? 0) + gapFor(e.similarity)
	}));

	const simulation = forceSimulation<SimNode, SimLink>(simNodes)
		.stop()
		.randomSource(seededRandom(seed))
		.force(
			'charge',
			forceManyBody<SimNode>()
				.strength((d) => (CHARGE_STRENGTH * d.radius) / DEFAULT_RADIUS)
				.distanceMin(CHARGE_DISTANCE_MIN)
		)
		.force(
			'link',
			forceLink<SimNode, SimLink>(simLinks)
				.id((d) => d.id)
				.distance((l) => l.restLength)
				.strength(LINK_STRENGTH)
		)
		.force('x', forceX<SimNode>(cx).strength(GRAVITY_STRENGTH))
		.force('y', forceY<SimNode>(cy).strength(GRAVITY_STRENGTH))
		.force('collide', forceRectCollide());

	return { simulation, simNodes, anchorId };
}

function snapshot(simNodes: readonly SimNode[]): NodePosition[] {
	return simNodes.map((n) => ({ id: n.id, x: n.x ?? 0, y: n.y ?? 0 }));
}

function settle(built: BuiltLayout, width: number, height: number): NodePosition[] {
	built.simulation.tick(ITERATIONS);

	for (let sweep = 0; sweep < COLLISION_SWEEPS; sweep++) {
		if (!resolveRectCollisions(built.simNodes)) break;
	}

	if (built.anchorId == null) {
		let minX = Infinity;
		let maxX = -Infinity;
		let minY = Infinity;
		let maxY = -Infinity;
		for (const n of built.simNodes) {
			const x = n.x ?? 0;
			const y = n.y ?? 0;
			minX = Math.min(minX, x - n.width / 2);
			maxX = Math.max(maxX, x + n.width / 2);
			minY = Math.min(minY, y - n.height / 2);
			maxY = Math.max(maxY, y + n.height / 2);
		}
		const offsetX = width / 2 - (minX + maxX) / 2;
		const offsetY = height / 2 - (minY + maxY) / 2;
		for (const n of built.simNodes) {
			n.x = (n.x ?? 0) + offsetX;
			n.y = (n.y ?? 0) + offsetY;
		}
	}

	return snapshot(built.simNodes);
}

export function layoutGraph(
	nodes: readonly LayoutNode[],
	edges: readonly LayoutEdge[],
	width: number,
	height: number,
	centreId?: string | null,
	seed = 0
): NodePosition[] {
	if (nodes.length === 0) return [];
	const built = buildSimulation(nodes, edges, width, height, centreId, seed);
	const positions = settle(built, width, height);
	built.simulation.stop();
	return positions;
}

export interface LayoutHandle {
	tick(): NodePosition[];
	positions(): NodePosition[];
	pin(id: string, x: number, y: number): void;
	unpin(id: string): void;
}

export function createLayoutSimulation(
	nodes: readonly LayoutNode[],
	edges: readonly LayoutEdge[],
	width: number,
	height: number,
	centreId?: string | null,
	seed = 0
): LayoutHandle {
	const built = buildSimulation(nodes, edges, width, height, centreId, seed);
	if (nodes.length > 0) settle(built, width, height);
	const byId = new Map(built.simNodes.map((n) => [n.id, n]));

	return {
		tick(): NodePosition[] {
			built.simulation.tick();
			return snapshot(built.simNodes);
		},
		positions(): NodePosition[] {
			return snapshot(built.simNodes);
		},
		pin(id: string, x: number, y: number): void {
			const n = byId.get(id);
			if (!n) return;
			n.fx = x;
			n.fy = y;
			n.x = x;
			n.y = y;
			built.simulation.alphaTarget(0.3);
			if (built.simulation.alpha() < 0.3) built.simulation.alpha(0.3);
		},
		unpin(id: string): void {
			const n = byId.get(id);
			if (!n) return;
			n.fx = null;
			n.fy = null;
			built.simulation.alphaTarget(0);
		}
	};
}
