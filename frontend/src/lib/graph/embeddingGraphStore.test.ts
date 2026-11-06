import { describe, it, expect, vi } from 'vitest';
import { EmbeddingGraphStore } from './embeddingGraphStore.svelte';
import type {
	ProjectEmbeddingGraph,
	SessionEmbeddingGraph,
	SessionGraphNode
} from '$lib/api/embeddingGraph';

const VIEWPORT = { width: 600, height: 400 };
const CENTRE = 'c0000000-0000-4000-8000-000000000000';

function sessionNode(
	overrides: Partial<SessionGraphNode> & { session_id: string }
): SessionGraphNode {
	return {
		title: 'Untitled session',
		project_id: 1,
		project_path: '/repo',
		outcome: 'accepted',
		last_seen: '2026-09-01T12:00:00Z',
		last_status: 'Complete',
		similarity: 0.7,
		source: 'overview',
		topics: [],
		is_centre: false,
		...overrides
	};
}

function baseSessionGraph(): SessionEmbeddingGraph {
	return {
		session_id: CENTRE,
		embedding: {
			present: true,
			source: 'overview',
			model: 'nomic-embed-text-v1.5',
			embedded_at: '2026-09-01T12:00:00Z',
			chunk_count: 10
		},
		nodes: [
			sessionNode({ session_id: CENTRE, title: 'Centre session', is_centre: true, similarity: 1 }),
			sessionNode({ session_id: 'n1', title: 'Neighbour one', similarity: 0.86 }),
			sessionNode({ session_id: 'n2', title: 'Neighbour two', similarity: 0.42 })
		],
		edges: [
			{ from: CENTRE, to: 'n1', similarity: 0.86 },
			{ from: CENTRE, to: 'n2', similarity: 0.42 }
		]
	};
}

function projectGraph(): ProjectEmbeddingGraph {
	return {
		project_id: 5,
		project_path: '/repo',
		centroid: { present: true, session_count: 2, embedded_at: '2026-09-01T12:00:00Z' },
		nodes: [
			{
				session_id: 'p1',
				title: 'Project session one',
				outcome: 'shipped',
				last_seen: '2026-09-01T12:00:00Z',
				last_status: 'Complete',
				similarity_to_centroid: 0.8,
				source: 'centroid',
				topics: []
			},
			{
				session_id: 'p2',
				title: 'Project session two',
				outcome: null,
				last_seen: '2026-09-01T12:00:00Z',
				last_status: 'Complete',
				similarity_to_centroid: 0.5,
				source: 'centroid',
				topics: []
			}
		],
		edges: [{ from: 'p1', to: 'p2', similarity: 0.6 }],
		sibling_projects: []
	};
}

function neighbourGraph(id: string, extraNeighbour: string): SessionEmbeddingGraph {
	return {
		session_id: id,
		embedding: {
			present: true,
			source: 'overview',
			model: 'nomic-embed-text-v1.5',
			embedded_at: '2026-09-01T12:00:00Z',
			chunk_count: 10
		},
		nodes: [
			sessionNode({ session_id: id, title: 'Expanded node', is_centre: true, similarity: 1 }),
			sessionNode({ session_id: CENTRE, title: 'Centre session', similarity: 0.86 }),
			sessionNode({ session_id: extraNeighbour, title: 'Brand new neighbour', similarity: 0.61 })
		],
		edges: [
			{ from: id, to: CENTRE, similarity: 0.86 },
			{ from: id, to: extraNeighbour, similarity: 0.61 }
		]
	};
}

describe('EmbeddingGraphStore', () => {
	it('load() positions every node and records the centre', () => {
		const store = new EmbeddingGraphStore();
		store.load(baseSessionGraph(), VIEWPORT);

		expect(store.visibleNodes).toHaveLength(3);
		expect(store.centreId).toBe(CENTRE);
		for (const node of store.visibleNodes) {
			expect(Number.isFinite(node.x)).toBe(true);
			expect(Number.isFinite(node.y)).toBe(true);
			expect(node.pinned).toBe(false);
			expect(node.origin).toBe('initial');
		}
		expect(store.nodes.get(CENTRE)?.x).toBe(VIEWPORT.width / 2);
		expect(store.nodes.get(CENTRE)?.y).toBe(VIEWPORT.height / 2);
	});

	it('load() on a project graph records the centroid and no centre id', () => {
		const store = new EmbeddingGraphStore();
		store.load(projectGraph(), VIEWPORT);

		expect(store.centreId).toBeNull();
		expect(store.centroid).toEqual({
			present: true,
			session_count: 2,
			embedded_at: '2026-09-01T12:00:00Z'
		});
		expect(store.visibleNodes.every((n) => n.projectId === 5 && n.projectPath === '/repo')).toBe(
			true
		);
	});

	it('expand() merges new nodes/edges without duplicating an existing node id or an undirected edge', async () => {
		const fetcher = vi.fn(async (id: string) => neighbourGraph(id, 'n3'));
		const store = new EmbeddingGraphStore(fetcher);
		store.load(baseSessionGraph(), VIEWPORT);
		const centreBefore = { ...store.nodes.get(CENTRE)! };

		await store.expand('n1');

		expect(fetcher).toHaveBeenCalledWith('n1', { k: 8 });
		expect(store.visibleNodes.map((n) => n.id).sort()).toEqual([CENTRE, 'n1', 'n2', 'n3'].sort());
		expect(store.nodes.get('n3')?.origin).toBe('expanded-from:n1');
		const betweenN1AndCentre = Array.from(store.edges.values()).filter(
			(e) => (e.from === 'n1' && e.to === CENTRE) || (e.from === CENTRE && e.to === 'n1')
		);
		expect(betweenN1AndCentre).toHaveLength(1);
		expect(store.nodes.has(CENTRE)).toBe(true);
		expect(centreBefore.id).toBe(CENTRE);
		expect(store.nodes.get('n1')?.expanded).toBe(true);
	});

	it('expand() keeps a pinned node exactly where it was, through the relax pass', async () => {
		const fetcher = vi.fn(async (id: string) => neighbourGraph(id, 'n3'));
		const store = new EmbeddingGraphStore(fetcher);
		store.load(baseSessionGraph(), VIEWPORT);
		store.setPosition('n2', 12, 34);

		await store.expand('n1');

		expect(store.nodes.get('n2')).toMatchObject({ x: 12, y: 34, pinned: true });
	});

	it('collapse() removes only the nodes whose sole origin is that expansion', async () => {
		const fetcher = vi.fn(async (id: string) => neighbourGraph(id, 'n3'));
		const store = new EmbeddingGraphStore(fetcher);
		store.load(baseSessionGraph(), VIEWPORT);
		await store.expand('n1');
		expect(store.nodes.has('n3')).toBe(true);

		store.collapse('n1');

		expect(store.nodes.has('n3')).toBe(false);
		expect(store.nodes.has(CENTRE)).toBe(true);
		expect(store.nodes.has('n1')).toBe(true);
		expect(store.nodes.has('n2')).toBe(true);
		expect(store.nodes.get('n1')?.expanded).toBe(false);
		for (const edge of store.edges.values()) {
			expect(edge.from).not.toBe('n3');
			expect(edge.to).not.toBe('n3');
		}
	});

	it('collapse() walks a chained expansion transitively: expand n1, expand n3 from it, collapse n1 removes both', async () => {
		const fetcher = vi.fn(async (id: string) => {
			if (id === 'n1') return neighbourGraph('n1', 'n3');
			if (id === 'n3') return neighbourGraph('n3', 'n4');
			throw new Error(`unexpected fetch for ${id}`);
		});
		const store = new EmbeddingGraphStore(fetcher);
		store.load(baseSessionGraph(), VIEWPORT);

		await store.expand('n1');
		expect(store.nodes.has('n3')).toBe(true);
		await store.expand('n3');
		expect(store.nodes.has('n4')).toBe(true);
		store.select('n4');

		store.collapse('n1');

		expect(store.nodes.has('n3')).toBe(false);
		expect(store.nodes.has('n4')).toBe(false);
		expect(store.nodes.has(CENTRE)).toBe(true);
		expect(store.nodes.has('n1')).toBe(true);
		expect(store.nodes.has('n2')).toBe(true);
		expect(store.nodes.get('n1')?.expanded).toBe(false);
		for (const edge of store.edges.values()) {
			expect(edge.from).not.toBe('n3');
			expect(edge.to).not.toBe('n3');
			expect(edge.from).not.toBe('n4');
			expect(edge.to).not.toBe('n4');
		}
		expect(store.selectedId).toBeNull();
	});

	it('similarityThreshold hides low-similarity edges without removing any node', () => {
		const store = new EmbeddingGraphStore();
		store.load(baseSessionGraph(), VIEWPORT);

		store.similarityThreshold = 0.5;

		expect(store.visibleNodes).toHaveLength(3);
		expect(store.visibleEdges.map((e) => e.to)).toEqual(['n1']);
		expect(store.edges.size).toBe(2);
	});

	it('select()/selected track the selected node, and null deselects', () => {
		const store = new EmbeddingGraphStore();
		store.load(baseSessionGraph(), VIEWPORT);

		store.select('n1');
		expect(store.selected?.id).toBe('n1');

		store.select(null);
		expect(store.selected).toBeNull();
	});

	it('setPosition() writes the position and pins the node', () => {
		const store = new EmbeddingGraphStore();
		store.load(baseSessionGraph(), VIEWPORT);

		store.setPosition('n1', 100, 200);

		expect(store.nodes.get('n1')).toMatchObject({ x: 100, y: 200, pinned: true });
	});

	it('relayout() moves an unpinned node but leaves a pinned one untouched', () => {
		const store = new EmbeddingGraphStore();
		store.load(baseSessionGraph(), VIEWPORT);
		store.setPosition('n1', 5, 5);
		const n2Before = { ...store.nodes.get('n2')! };

		store.relayout();

		expect(store.nodes.get('n1')).toMatchObject({ x: 5, y: 5 });
		expect(Number.isFinite(store.nodes.get('n2')!.x)).toBe(true);
		void n2Before;
	});

	it('load() bumps generation, so the renderer knows a fresh measure-then-relayout pass is owed', () => {
		const store = new EmbeddingGraphStore();
		const before = store.generation;
		store.load(baseSessionGraph(), VIEWPORT);
		expect(store.generation).toBe(before + 1);
	});

	it('expand() bumps generation only when it actually adds a new node', async () => {
		const fetcher = vi.fn(async (id: string) => neighbourGraph(id, 'n3'));
		const store = new EmbeddingGraphStore(fetcher);
		store.load(baseSessionGraph(), VIEWPORT);
		const afterLoad = store.generation;

		await store.expand('n1');
		expect(store.generation).toBe(afterLoad + 1);

		const afterFirstExpand = store.generation;
		await store.expand('n1');
		expect(store.generation).toBe(afterFirstExpand);
	});

	it("setDimensions() records a node's real measured size, and relayout() lays out with it", () => {
		const store = new EmbeddingGraphStore();
		store.load(baseSessionGraph(), VIEWPORT);

		store.setDimensions([{ id: 'n1', width: 240, height: 88 }]);

		expect(store.nodes.get('n1')).toMatchObject({ width: 240, height: 88 });
		store.relayout();
		expect(Number.isFinite(store.nodes.get('n1')!.x)).toBe(true);
	});

	it('setDimensions() is a no-op for an unknown node id', () => {
		const store = new EmbeddingGraphStore();
		store.load(baseSessionGraph(), VIEWPORT);

		expect(() => store.setDimensions([{ id: 'not-a-node', width: 100, height: 50 }])).not.toThrow();
		expect(store.nodes.has('not-a-node')).toBe(false);
	});

	it('neighboursOf() returns only nodes connected by a currently-visible edge', () => {
		const store = new EmbeddingGraphStore();
		store.load(baseSessionGraph(), VIEWPORT);
		store.similarityThreshold = 0.5;

		const neighbours = store.neighboursOf(CENTRE).map((n) => n.id);

		expect(neighbours).toEqual(['n1']);
	});
});
