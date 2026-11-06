import { describe, it, expect } from 'vitest';
import {
	layoutGraph,
	createLayoutSimulation,
	DEFAULT_NODE_WIDTH,
	DEFAULT_NODE_HEIGHT,
	type LayoutEdge,
	type LayoutNode
} from './forceLayout';

const WIDTH = 600;
const HEIGHT = 400;

function star(n: number): { nodes: LayoutNode[]; edges: LayoutEdge[] } {
	const nodes: LayoutNode[] = [{ id: 'centre' }];
	for (let i = 0; i < n; i++) nodes.push({ id: `n${i}` });
	const edges: LayoutEdge[] = nodes
		.slice(1)
		.map((node, i) => ({ from: 'centre', to: node.id, similarity: 0.3 + (i % 5) * 0.1 }));
	return { nodes, edges };
}

function realisticSize(i: number): { width: number; height: number } {
	return { width: 150 + ((i * 37) % 111), height: 40 + ((i * 13) % 51) };
}

function realisticGraph(n: number): { nodes: LayoutNode[]; edges: LayoutEdge[] } {
	const nodes: LayoutNode[] = [{ id: 'centre', ...realisticSize(0) }];
	for (let i = 1; i < n; i++) nodes.push({ id: `n${i}`, ...realisticSize(i) });
	const edges: LayoutEdge[] = [];
	for (let i = 1; i < n; i++) {
		edges.push({ from: 'centre', to: `n${i}`, similarity: 0.3 + (i % 6) * 0.1 });
		if (i > 1) edges.push({ from: `n${i - 1}`, to: `n${i}`, similarity: 0.2 + (i % 4) * 0.1 });
	}
	return { nodes, edges };
}

function assertNoOverlaps(nodes: LayoutNode[], positions: ReturnType<typeof layoutGraph>): void {
	const posById = new Map(positions.map((p) => [p.id, p]));
	const TOLERANCE = 0.5;
	const offenders: string[] = [];
	for (let i = 0; i < nodes.length; i++) {
		for (let j = i + 1; j < nodes.length; j++) {
			const a = nodes[i]!;
			const b = nodes[j]!;
			const pa = posById.get(a.id)!;
			const pb = posById.get(b.id)!;
			const aw = a.width ?? DEFAULT_NODE_WIDTH;
			const ah = a.height ?? DEFAULT_NODE_HEIGHT;
			const bw = b.width ?? DEFAULT_NODE_WIDTH;
			const bh = b.height ?? DEFAULT_NODE_HEIGHT;
			const overlapX = aw / 2 + bw / 2 - Math.abs(pb.x - pa.x);
			const overlapY = ah / 2 + bh / 2 - Math.abs(pb.y - pa.y);
			if (overlapX > TOLERANCE && overlapY > TOLERANCE) {
				offenders.push(
					`${a.id}<->${b.id} overlaps by (${overlapX.toFixed(1)}, ${overlapY.toFixed(1)})`
				);
			}
		}
	}
	expect(offenders).toEqual([]);
}

describe('layoutGraph', () => {
	it('is deterministic — identical input produces identical output', () => {
		const { nodes, edges } = star(6);
		const first = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		const second = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		expect(second).toEqual(first);
	});

	it('is deterministic with realistic per-node card sizes too', () => {
		const { nodes, edges } = realisticGraph(12);
		const first = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		const second = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		expect(second).toEqual(first);
	});

	it('returns one position per node, in input order', () => {
		const { nodes, edges } = star(4);
		const positions = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		expect(positions.map((p) => p.id)).toEqual(nodes.map((n) => n.id));
	});

	it('returns an empty array for an empty node list', () => {
		expect(layoutGraph([], [], WIDTH, HEIGHT, null)).toEqual([]);
	});

	it('keeps every position finite regardless of graph shape', () => {
		const { nodes, edges } = star(12);
		const positions = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		for (const p of positions) {
			expect(Number.isFinite(p.x)).toBe(true);
			expect(Number.isFinite(p.y)).toBe(true);
		}
	});

	it('pins the centre node at the exact canvas centre', () => {
		const { nodes, edges } = star(8);
		const positions = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		const centre = positions.find((p) => p.id === 'centre');
		expect(centre).toEqual({ id: 'centre', x: WIDTH / 2, y: HEIGHT / 2 });
	});

	it('pins the centre node at the exact canvas centre with realistic card sizes and a working canvas larger than the requested viewport', () => {
		const { nodes, edges } = realisticGraph(12);
		const positions = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		const centre = positions.find((p) => p.id === 'centre');
		expect(centre).toEqual({ id: 'centre', x: WIDTH / 2, y: HEIGHT / 2 });
	});

	it('places a higher-similarity edge endpoint closer to the centre than a lower-similarity one', () => {
		const nodes: LayoutNode[] = [{ id: 'centre' }, { id: 'A' }, { id: 'B' }];
		const edges: LayoutEdge[] = [
			{ from: 'centre', to: 'A', similarity: 0.95 },
			{ from: 'centre', to: 'B', similarity: 0.1 }
		];
		const positions = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		const byId = new Map(positions.map((p) => [p.id, p]));
		const centre = byId.get('centre')!;
		const a = byId.get('A')!;
		const b = byId.get('B')!;
		const distA = Math.hypot(a.x - centre.x, a.y - centre.y);
		const distB = Math.hypot(b.x - centre.x, b.y - centre.y);
		expect(distA).toBeLessThan(distB);
	});

	it('places a higher-similarity edge endpoint closer than a lower-similarity one when neither is in collision (realistic card sizes)', () => {
		const nodes: LayoutNode[] = [
			{ id: 'centre', width: 200, height: 64 },
			{ id: 'A', width: 180, height: 56 },
			{ id: 'B', width: 220, height: 72 }
		];
		const edges: LayoutEdge[] = [
			{ from: 'centre', to: 'A', similarity: 0.95 },
			{ from: 'centre', to: 'B', similarity: 0.1 }
		];
		const positions = layoutGraph(nodes, edges, 900, 700, 'centre');
		const byId = new Map(positions.map((p) => [p.id, p]));
		const centre = byId.get('centre')!;
		const a = byId.get('A')!;
		const b = byId.get('B')!;
		const distA = Math.hypot(a.x - centre.x, a.y - centre.y);
		const distB = Math.hypot(b.x - centre.x, b.y - centre.y);
		expect(distA).toBeLessThan(distB);
	});

	it('ignores edges referencing an id not present in nodes, and self-loops', () => {
		const nodes: LayoutNode[] = [{ id: 'centre' }, { id: 'A' }];
		const edges: LayoutEdge[] = [
			{ from: 'centre', to: 'A', similarity: 0.5 },
			{ from: 'centre', to: 'ghost', similarity: 0.9 },
			{ from: 'A', to: 'A', similarity: 1 }
		];
		expect(() => layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre')).not.toThrow();
	});

	it('places a single non-centre node without dividing by zero', () => {
		const nodes: LayoutNode[] = [{ id: 'only' }];
		const positions = layoutGraph(nodes, [], WIDTH, HEIGHT, null);
		expect(positions).toHaveLength(1);
		expect(Number.isFinite(positions[0]!.x)).toBe(true);
		expect(Number.isFinite(positions[0]!.y)).toBe(true);
	});

	it('resolves a 12-node graph with realistic card sizes to zero overlapping pairs', () => {
		const { nodes, edges } = realisticGraph(12);
		const positions = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre');
		assertNoOverlaps(nodes, positions);
	});

	it('resolves a 60-node graph with realistic card sizes to zero overlapping pairs', () => {
		const { nodes, edges } = realisticGraph(60);
		const positions = layoutGraph(nodes, edges, 960, 620, 'centre');
		assertNoOverlaps(nodes, positions);
	});

	it('resolves overlaps with no centre node too (a project graph, centred on its centroid instead)', () => {
		const { nodes, edges } = realisticGraph(16);
		const positions = layoutGraph(nodes, edges, WIDTH, HEIGHT, null);
		assertNoOverlaps(nodes, positions);
	});

	it('gives a graph needing more room than the requested viewport a bigger working canvas rather than squashing cards back into overlap', () => {
		const { nodes, edges } = realisticGraph(40);
		const positions = layoutGraph(nodes, edges, 300, 200, 'centre');
		assertNoOverlaps(nodes, positions);
	});

	it('seed is deterministic — the same seed reproduces the same layout', () => {
		const { nodes, edges } = realisticGraph(10);
		const first = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre', 3);
		const second = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre', 3);
		expect(second).toEqual(first);
	});

	it('a different seed converges to a genuinely different arrangement (the "Relayout" button contract)', () => {
		const { nodes, edges } = realisticGraph(10);
		const seed0 = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre', 0);
		const seed1 = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre', 1);
		const seed0ById = new Map(seed0.map((p) => [p.id, p]));
		const moved = seed1.some((p) => {
			if (p.id === 'centre') return false;
			const before = seed0ById.get(p.id)!;
			return Math.hypot(p.x - before.x, p.y - before.y) > 10;
		});
		expect(moved).toBe(true);
	});

	it('the seeded arrangement still resolves to zero overlap', () => {
		const { nodes, edges } = realisticGraph(12);
		const positions = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre', 7);
		assertNoOverlaps(nodes, positions);
	});

	it('is byte-identical across two independent calls with the same seed — not merely close', () => {
		const { nodes, edges } = realisticGraph(24);
		const first = layoutGraph(nodes, edges, 960, 620, 'centre', 5);
		const second = layoutGraph(nodes, edges, 960, 620, 'centre', 5);
		expect(first).toHaveLength(second.length);
		for (let i = 0; i < first.length; i++) {
			expect(Object.is(first[i]!.x, second[i]!.x)).toBe(true);
			expect(Object.is(first[i]!.y, second[i]!.y)).toBe(true);
		}
		expect(JSON.stringify(first)).toBe(JSON.stringify(second));
	});

	describe('createLayoutSimulation (live handle for continuous drag — embedding-graph-v2 addendum §4)', () => {
		it('starts from exactly the arrangement layoutGraph itself would return for the same input', () => {
			const { nodes, edges } = realisticGraph(14);
			const expected = layoutGraph(nodes, edges, WIDTH, HEIGHT, 'centre', 2);
			const handle = createLayoutSimulation(nodes, edges, WIDTH, HEIGHT, 'centre', 2);
			expect(handle.positions()).toEqual(expected);
		});

		it('pinning a node holds it exactly at the pinned point across further ticks', () => {
			const { nodes, edges } = realisticGraph(10);
			const handle = createLayoutSimulation(nodes, edges, WIDTH, HEIGHT, 'centre', 0);
			handle.pin('n3', 50, 340);
			for (let i = 0; i < 20; i++) {
				const positions = handle.tick();
				const pinned = positions.find((p) => p.id === 'n3')!;
				expect(pinned.x).toBe(50);
				expect(pinned.y).toBe(340);
			}
		});

		it('dragging a node toward a neighbour displaces that neighbour clear rather than letting them overlap', () => {
			const { nodes, edges } = realisticGraph(10);
			const handle = createLayoutSimulation(nodes, edges, WIDTH, HEIGHT, 'centre', 0);
			const before = new Map(handle.positions().map((p) => [p.id, p]));
			const target = before.get('n2')!;
			handle.pin('n5', target.x, target.y);
			let positions = handle.positions();
			for (let i = 0; i < 30; i++) positions = handle.tick();
			const n2After = positions.find((p) => p.id === 'n2')!;
			const n5After = positions.find((p) => p.id === 'n5')!;
			const n2Node = nodes.find((n) => n.id === 'n2')!;
			const n5Node = nodes.find((n) => n.id === 'n5')!;
			const overlapX =
				(n2Node.width ?? DEFAULT_NODE_WIDTH) / 2 +
				(n5Node.width ?? DEFAULT_NODE_WIDTH) / 2 -
				Math.abs(n2After.x - n5After.x);
			const overlapY =
				(n2Node.height ?? DEFAULT_NODE_HEIGHT) / 2 +
				(n5Node.height ?? DEFAULT_NODE_HEIGHT) / 2 -
				Math.abs(n2After.y - n5After.y);
			expect(overlapX <= 0.5 || overlapY <= 0.5).toBe(true);
		});

		it('unpinning releases the node back to the simulation (its position is no longer forced to the pinned point after further ticks change everything else)', () => {
			const { nodes, edges } = realisticGraph(6);
			const handle = createLayoutSimulation(nodes, edges, WIDTH, HEIGHT, 'centre', 0);
			handle.pin('n1', 5, 5);
			handle.tick();
			handle.unpin('n1');
			handle.pin('n2', 400, 300);
			let positions = handle.positions();
			for (let i = 0; i < 40; i++) positions = handle.tick();
			const n1After = positions.find((p) => p.id === 'n1')!;
			expect(n1After.x === 5 && n1After.y === 5).toBe(false);
		});
	});
});
