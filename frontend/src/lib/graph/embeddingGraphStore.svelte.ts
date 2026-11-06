import { SvelteMap } from 'svelte/reactivity';
import { layoutGraph, type LayoutEdge, type LayoutNode } from './forceLayout';
import {
	getSessionEmbeddingGraph,
	type EmbeddingSource,
	type GetSessionEmbeddingGraphParams,
	type ProjectCentroidStatus,
	type ProjectEmbeddingGraph,
	type SessionEmbeddingGraph
} from '$lib/api/embeddingGraph';

export type GraphNodeOrigin = 'initial' | `expanded-from:${string}`;

export interface GraphNode {
	id: string;
	title: string;
	outcome: string | null;
	lastSeen: string;
	lastStatus: string;
	similarity: number;
	topics: string[];
	isCentre: boolean;
	projectId: number | null;
	projectPath: string | null;
	source: EmbeddingSource;
	x: number;
	y: number;
	width?: number;
	height?: number;
	pinned: boolean;
	expanded: boolean;
	origin: GraphNodeOrigin;
}

export interface GraphEdge {
	id: string;
	from: string;
	to: string;
	similarity: number;
}

export type SessionGraphFetcher = (
	sessionId: string,
	params?: GetSessionEmbeddingGraphParams
) => Promise<SessionEmbeddingGraph>;

export interface GraphViewport {
	width: number;
	height: number;
}

const EXPAND_K = 8;
const EXPAND_SEED_RADIUS = 90;
const DEFAULT_VIEWPORT: GraphViewport = { width: 960, height: 560 };
const DEFAULT_SIMILARITY_THRESHOLD = 0.3;

function edgeKey(a: string, b: string): string {
	return a < b ? `${a}::${b}` : `${b}::${a}`;
}

function isSessionGraph(
	graph: SessionEmbeddingGraph | ProjectEmbeddingGraph
): graph is SessionEmbeddingGraph {
	return 'embedding' in graph;
}

export class EmbeddingGraphStore {
	nodes = new SvelteMap<string, GraphNode>();
	edges = new SvelteMap<string, GraphEdge>();

	selectedId = $state<string | null>(null);
	similarityThreshold = $state(DEFAULT_SIMILARITY_THRESHOLD);
	isLoading = $state(false);
	error = $state<string | null>(null);
	centroid = $state<ProjectCentroidStatus | null>(null);
	centreId = $state<string | null>(null);
	generation = $state(0);

	#viewport: GraphViewport = DEFAULT_VIEWPORT;
	#fetchSessionGraph: SessionGraphFetcher;

	constructor(fetchSessionGraph: SessionGraphFetcher = getSessionEmbeddingGraph) {
		this.#fetchSessionGraph = fetchSessionGraph;
	}

	get viewport(): GraphViewport {
		return this.#viewport;
	}

	readonly visibleNodes = $derived(Array.from(this.nodes.values()));
	readonly visibleEdges = $derived(
		Array.from(this.edges.values()).filter(
			(edge) =>
				edge.similarity >= this.similarityThreshold &&
				this.nodes.has(edge.from) &&
				this.nodes.has(edge.to)
		)
	);
	readonly selected = $derived(
		this.selectedId != null ? (this.nodes.get(this.selectedId) ?? null) : null
	);

	neighboursOf(id: string): GraphNode[] {
		const result: GraphNode[] = [];
		for (const edge of this.visibleEdges) {
			if (edge.from === id) {
				const n = this.nodes.get(edge.to);
				if (n) result.push(n);
			} else if (edge.to === id) {
				const n = this.nodes.get(edge.from);
				if (n) result.push(n);
			}
		}
		return result;
	}

	load(
		graph: SessionEmbeddingGraph | ProjectEmbeddingGraph,
		viewport: GraphViewport = DEFAULT_VIEWPORT
	): void {
		this.#viewport = viewport;
		this.nodes.clear();
		this.edges.clear();
		this.selectedId = null;
		this.error = null;
		this.similarityThreshold = DEFAULT_SIMILARITY_THRESHOLD;

		const sessionGraph = isSessionGraph(graph);
		this.centreId = sessionGraph ? graph.session_id : null;
		this.centroid = sessionGraph ? null : graph.centroid;

		const layoutNodes: LayoutNode[] = graph.nodes.map((n) => ({ id: n.session_id }));
		const layoutEdges: LayoutEdge[] = graph.edges.map((e) => ({
			from: e.from,
			to: e.to,
			similarity: e.similarity
		}));
		const positions = layoutGraph(
			layoutNodes,
			layoutEdges,
			viewport.width,
			viewport.height,
			this.centreId
		);
		const posById = new Map(positions.map((p) => [p.id, p]));

		if (sessionGraph) {
			for (const n of graph.nodes) {
				const pos = posById.get(n.session_id) ?? { x: viewport.width / 2, y: viewport.height / 2 };
				this.nodes.set(n.session_id, {
					id: n.session_id,
					title: n.title,
					outcome: n.outcome,
					lastSeen: n.last_seen,
					lastStatus: n.last_status ?? '',
					similarity: n.similarity,
					topics: n.topics,
					isCentre: n.is_centre,
					projectId: n.project_id,
					projectPath: n.project_path,
					source: n.source,
					x: pos.x,
					y: pos.y,
					pinned: false,
					expanded: false,
					origin: 'initial'
				});
			}
		} else {
			for (const n of graph.nodes) {
				const pos = posById.get(n.session_id) ?? { x: viewport.width / 2, y: viewport.height / 2 };
				this.nodes.set(n.session_id, {
					id: n.session_id,
					title: n.title,
					outcome: n.outcome,
					lastSeen: n.last_seen,
					lastStatus: n.last_status ?? '',
					similarity: n.similarity_to_centroid,
					topics: n.topics,
					isCentre: false,
					projectId: graph.project_id,
					projectPath: graph.project_path,
					source: n.source,
					x: pos.x,
					y: pos.y,
					pinned: false,
					expanded: false,
					origin: 'initial'
				});
			}
		}
		for (const e of graph.edges) {
			const key = edgeKey(e.from, e.to);
			this.edges.set(key, { id: key, from: e.from, to: e.to, similarity: e.similarity });
		}
		this.generation += 1;
	}

	select(id: string | null): void {
		this.selectedId = id;
	}

	async expand(id: string): Promise<void> {
		const centre = this.nodes.get(id);
		if (!centre) return;
		this.isLoading = true;
		this.error = null;
		try {
			const graph = await this.#fetchSessionGraph(id, { k: EXPAND_K });
			const newNodes = graph.nodes.filter(
				(n) => n.session_id !== id && !this.nodes.has(n.session_id)
			);

			newNodes.forEach((n, i) => {
				const angle = (2 * Math.PI * i) / Math.max(1, newNodes.length);
				this.nodes.set(n.session_id, {
					id: n.session_id,
					title: n.title,
					outcome: n.outcome,
					lastSeen: n.last_seen,
					lastStatus: n.last_status ?? '',
					similarity: n.similarity,
					topics: n.topics,
					isCentre: false,
					projectId: n.project_id,
					projectPath: n.project_path,
					source: n.source,
					x: centre.x + Math.cos(angle) * EXPAND_SEED_RADIUS,
					y: centre.y + Math.sin(angle) * EXPAND_SEED_RADIUS,
					pinned: false,
					expanded: false,
					origin: `expanded-from:${id}`
				});
			});

			for (const e of graph.edges) {
				const key = edgeKey(e.from, e.to);
				if (this.edges.has(key)) continue;
				if (!this.nodes.has(e.from) || !this.nodes.has(e.to)) continue;
				this.edges.set(key, { id: key, from: e.from, to: e.to, similarity: e.similarity });
			}

			this.#updateNode(id, { expanded: true });
			this.#relax();
			if (newNodes.length > 0) this.generation += 1;
		} catch (err) {
			this.error = err instanceof Error ? err.message : String(err);
		} finally {
			this.isLoading = false;
		}
	}

	collapse(id: string): void {
		const toRemove = new Set<string>();
		const frontier: string[] = [id];
		while (frontier.length > 0) {
			const originId = frontier.pop()!;
			const originTag: GraphNodeOrigin = `expanded-from:${originId}`;
			for (const [nodeId, node] of this.nodes) {
				if (node.origin === originTag && !toRemove.has(nodeId)) {
					toRemove.add(nodeId);
					frontier.push(nodeId);
				}
			}
		}
		for (const nodeId of toRemove) this.nodes.delete(nodeId);
		for (const [key, edge] of this.edges) {
			if (!this.nodes.has(edge.from) || !this.nodes.has(edge.to)) this.edges.delete(key);
		}
		if (this.nodes.has(id)) this.#updateNode(id, { expanded: false });
		if (this.selectedId != null && !this.nodes.has(this.selectedId)) this.selectedId = null;
	}

	pin(id: string): void {
		this.#updateNode(id, { pinned: true });
	}

	unpin(id: string): void {
		this.#updateNode(id, { pinned: false });
	}

	setPosition(id: string, x: number, y: number): void {
		this.#updateNode(id, { x, y, pinned: true });
	}

	setPositionLive(id: string, x: number, y: number): void {
		this.#updateNode(id, { x, y });
	}

	relayout(options?: { seed?: number; unpinAll?: boolean }): void {
		const { seed = 0, unpinAll = false } = options ?? {};
		if (unpinAll) {
			for (const node of this.nodes.values()) {
				if (node.pinned) this.#updateNode(node.id, { pinned: false });
			}
		}
		this.#relax(seed);
	}

	setDimensions(entries: ReadonlyArray<{ id: string; width: number; height: number }>): void {
		for (const { id, width, height } of entries) {
			const node = this.nodes.get(id);
			if (!node) continue;
			if (node.width === width && node.height === height) continue;
			this.#updateNode(id, { width, height });
		}
	}

	#updateNode(id: string, patch: Partial<GraphNode>): void {
		const existing = this.nodes.get(id);
		if (!existing) return;
		this.nodes.set(id, { ...existing, ...patch });
	}

	#relax(seed = 0): void {
		const layoutNodes: LayoutNode[] = Array.from(this.nodes.values(), (n) => ({
			id: n.id,
			width: n.width,
			height: n.height
		}));
		const layoutEdges: LayoutEdge[] = Array.from(this.edges.values(), (e) => ({
			from: e.from,
			to: e.to,
			similarity: e.similarity
		}));
		const positions = layoutGraph(
			layoutNodes,
			layoutEdges,
			this.#viewport.width,
			this.#viewport.height,
			this.centreId,
			seed
		);
		const posById = new Map(positions.map((p) => [p.id, p]));
		for (const node of this.nodes.values()) {
			if (node.pinned) continue;
			const pos = posById.get(node.id);
			if (pos) this.#updateNode(node.id, { x: pos.x, y: pos.y });
		}
	}
}
