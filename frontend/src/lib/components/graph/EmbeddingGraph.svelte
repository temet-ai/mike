<script module lang="ts">
	export interface EmbeddingGraphProps {
		store: EmbeddingGraphStore;
		mode: 'compact' | 'full';
		onOpenSession: (sessionId: string) => void;
		expandHref?: string;
		collapseHref?: string;
		fetchSession?: SessionFetcher;
	}
</script>

<script lang="ts">
	import { tick } from 'svelte';
	import '@xyflow/svelte/dist/style.css';
	import {
		SvelteFlow,
		Background,
		BackgroundVariant,
		Controls,
		MiniMap,
		Panel,
		type Node,
		type Edge,
		type NodeTargetEventWithPointer
	} from '@xyflow/svelte';
	import { UnfoldHorizontal, FoldHorizontal, Locate, RotateCcw } from '@lucide/svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Slider } from '$lib/components/ui/slider';
	import SessionFlowNode, {
		type SessionFlowNodeData
	} from '$lib/components/graph/nodes/SessionFlowNode.svelte';
	import FlowController from '$lib/components/graph/FlowController.svelte';
	import SessionPreviewModal, {
		type SessionPreviewNode,
		type SessionFetcher
	} from '$lib/components/graph/SessionPreviewModal.svelte';
	import {
		EmbeddingGraphStore,
		type GraphNode,
		type GraphEdge
	} from '$lib/graph/embeddingGraphStore.svelte';
	import {
		createLayoutSimulation,
		type LayoutHandle,
		type LayoutNode,
		type LayoutEdge
	} from '$lib/graph/forceLayout';
	import { graphOutcomeColorVar, graphOutcomeLabel } from '$lib/components/graph/outcomeColors';
	import { formatRelative, formatClockAbsolute } from '$lib/time';
	import { deriveSessionActivity } from '$lib/sessionActivity';
	import type { SessionSummary } from '$lib/types';

	let { store, mode, onOpenSession, expandHref, collapseHref, fetchSession }: EmbeddingGraphProps =
		$props();

	const nodeTypes = { session: SessionFlowNode };

	const visibleNodes = $derived(Array.from(store.nodes.values()));
	const visibleEdges = $derived(
		Array.from(store.edges.values()).filter(
			(edge) =>
				edge.similarity >= store.similarityThreshold &&
				store.nodes.has(edge.from) &&
				store.nodes.has(edge.to)
		)
	);
	const selected = $derived(
		store.selectedId != null ? (store.nodes.get(store.selectedId) ?? null) : null
	);

	let flowNodes = $state.raw<Node[]>([]);
	let flowEdges = $state.raw<Edge[]>([]);
	let doFit: (() => void) | null = null;
	let relayoutPresses = $state(0);
	let autoRelayoutedGeneration = $state<number | null>(null);
	let fittedGeneration = $state<number | null>(null);
	let canvasEl: HTMLElement | undefined = undefined;

	let hoveredId = $state<string | null>(null);
	const activeId = $derived(hoveredId ?? store.selectedId);
	const incidentIds = $derived.by(() => {
		if (activeId == null) return null;
		const ids = new Set<string>([activeId]);
		for (const edge of visibleEdges) {
			if (edge.from === activeId) ids.add(edge.to);
			else if (edge.to === activeId) ids.add(edge.from);
		}
		return ids;
	});

	let previewNode = $state<SessionPreviewNode | null>(null);
	let previewOriginEl = $state<HTMLElement | null>(null);

	function openPreview(id: string, originEl: HTMLElement): void {
		const node = store.nodes.get(id);
		if (!node) return;
		previewNode = {
			id: node.id,
			title: node.title,
			outcome: node.outcome,
			similarity: node.similarity
		};
		previewOriginEl = originEl;
	}
	function closePreview(): void {
		previewNode = null;
		previewOriginEl = null;
	}

	function toFlowNode(node: GraphNode): Node {
		const data: SessionFlowNodeData = {
			node,
			mode,
			isSelected: store.selectedId === node.id,
			isDimmed: incidentIds != null && !incidentIds.has(node.id),
			isIncident: incidentIds != null && incidentIds.has(node.id),
			onSelect: (id) => store.select(id),
			onExpand: (id) => void handleExpand(id),
			onOpenPreview: (id, originEl) => openPreview(id, originEl),
			onHoverStart: (id) => (hoveredId = id),
			onHoverEnd: (id) => {
				if (hoveredId === id) hoveredId = null;
			}
		};
		return {
			id: node.id,
			type: 'session',
			position: { x: node.x, y: node.y },
			focusable: false,
			draggable: true,
			width: node.width,
			height: node.height,
			data
		};
	}

	function edgeOpacityBand(nodeCount: number): { min: number; max: number } {
		if (nodeCount <= 8) return { min: 0.2, max: 0.65 };
		if (nodeCount <= 30) return { min: 0.08, max: 0.38 };
		return { min: 0.05, max: 0.25 };
	}

	const HIGHLIGHT_OPACITY = 0.9;
	const HIGHLIGHT_WIDTH_BOOST = 1;

	function toActivityInput(node: GraphNode): SessionSummary {
		return {
			id: node.id,
			project: node.projectPath,
			project_id: node.projectId,
			cwd: null,
			project_resolution: 'unresolved',
			first_seen: node.lastSeen,
			last_seen: node.lastSeen,
			request_count: 0,
			models: [],
			last_status: node.lastStatus,
			title: node.title
		};
	}

	function isNodeActive(node: GraphNode, now: number): boolean {
		return deriveSessionActivity(toActivityInput(node), null, now).isActive;
	}

	function edgeStyle(
		edge: GraphEdge,
		nodeCount: number,
		active: string | null,
		now: number
	): string {
		const clamped = Math.min(1, Math.max(0, edge.similarity));
		const isCentreEdge =
			store.centreId != null && (edge.from === store.centreId || edge.to === store.centreId);
		const isIncident = active != null && (edge.from === active || edge.to === active);

		let width = 1 + clamped * (nodeCount > 30 ? 1.2 : 1.5);
		let opacity: number;
		let stroke = '';

		if (isCentreEdge) {
			opacity = 0.15 + clamped * 0.25;
			stroke = ';stroke:var(--funk-cyan)';
		} else {
			const { min, max } = edgeOpacityBand(nodeCount);
			opacity = min + clamped * (max - min);
		}

		if (isIncident) {
			opacity = HIGHLIGHT_OPACITY;
			width += HIGHLIGHT_WIDTH_BOOST;
			const otherId = edge.from === active ? edge.to : edge.from;
			const otherNode = store.nodes.get(otherId);
			const otherIsActive = otherNode != null && isNodeActive(otherNode, now);
			stroke = otherIsActive ? ';stroke:var(--funk-magenta)' : ';stroke:var(--funk-cyan)';
		}

		return `stroke-width:${width}px;opacity:${Math.min(1, opacity)}${stroke}`;
	}

	$effect(() => {
		flowNodes = visibleNodes.map(toFlowNode);
	});
	$effect(() => {
		const nodeCount = visibleNodes.length;
		const active = activeId;
		const now = Date.now();
		flowEdges = visibleEdges.map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			type: 'default',
			selectable: false,
			focusable: false,
			zIndex: -1,
			style: edgeStyle(edge, nodeCount, active, now)
		}));
	});

	async function handleExpand(id: string): Promise<void> {
		await store.expand(id);
		void fitAfterRelayout();
	}

	async function handleRelayout(): Promise<void> {
		relayoutPresses += 1;
		store.relayout({ seed: relayoutPresses, unpinAll: true });
		await fitAfterRelayout();
	}

	async function fitAfterRelayout(): Promise<void> {
		await tick();
		doFit?.();
	}

	$effect(() => {
		const generation = store.generation;
		if (generation === autoRelayoutedGeneration) return;
		const currentNodes = flowNodes;
		if (currentNodes.length === 0) {
			autoRelayoutedGeneration = generation;
			return;
		}
		const allMeasured = currentNodes.every(
			(n) => (n.measured?.width ?? 0) > 0 && (n.measured?.height ?? 0) > 0
		);
		if (!allMeasured) return;

		autoRelayoutedGeneration = generation;
		store.setDimensions(
			currentNodes.map((n) => ({
				id: n.id,
				width: n.measured!.width!,
				height: n.measured!.height!
			}))
		);
		store.relayout();
		void fitAfterRelayout();
	});

	function cardsFitWithinCanvas(): boolean {
		if (!canvasEl) return true;
		const containerRect = canvasEl.getBoundingClientRect();
		const cards = canvasEl.querySelectorAll('[data-slot="embedding-graph-node"]');
		const TOLERANCE = 2;
		for (const card of cards) {
			const rect = card.getBoundingClientRect();
			const fits =
				rect.left >= containerRect.left - TOLERANCE &&
				rect.right <= containerRect.right + TOLERANCE &&
				rect.top >= containerRect.top - TOLERANCE &&
				rect.bottom <= containerRect.bottom + TOLERANCE;
			if (!fits) return false;
		}
		return true;
	}

	async function fitUntilContained(): Promise<void> {
		const MAX_ATTEMPTS = 12;
		const RETRY_DELAY_MS = 500;
		for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
			await tick();
			doFit?.();
			await new Promise((resolve) => setTimeout(resolve, RETRY_DELAY_MS));
			if (cardsFitWithinCanvas()) return;
		}
	}

	$effect(() => {
		const generation = store.generation;
		if (generation === fittedGeneration) return;
		if (flowNodes.length === 0) return;
		fittedGeneration = generation;
		void fitUntilContained();
	});

	let dragSimulation: LayoutHandle | null = null;

	const onnodedragstart: NodeTargetEventWithPointer<MouseEvent | TouchEvent> = ({ targetNode }) => {
		if (!targetNode) return;
		const layoutNodes: LayoutNode[] = Array.from(store.nodes.values(), (n) => ({
			id: n.id,
			width: n.width,
			height: n.height
		}));
		const layoutEdges: LayoutEdge[] = Array.from(store.edges.values(), (e) => ({
			from: e.from,
			to: e.to,
			similarity: e.similarity
		}));
		dragSimulation = createLayoutSimulation(
			layoutNodes,
			layoutEdges,
			store.viewport.width,
			store.viewport.height,
			store.centreId
		);
		for (const node of store.nodes.values()) {
			if (node.pinned && node.id !== targetNode.id) {
				dragSimulation.pin(node.id, node.x, node.y);
			}
		}
		dragSimulation.pin(targetNode.id, targetNode.position.x, targetNode.position.y);
	};

	const onnodedrag: NodeTargetEventWithPointer<MouseEvent | TouchEvent> = ({ targetNode }) => {
		if (!targetNode || !dragSimulation) return;
		dragSimulation.pin(targetNode.id, targetNode.position.x, targetNode.position.y);
		for (const p of dragSimulation.tick()) {
			store.setPositionLive(p.id, p.x, p.y);
		}
	};

	const onnodedragstop: NodeTargetEventWithPointer<MouseEvent | TouchEvent> = ({ targetNode }) => {
		if (!targetNode) return;
		dragSimulation?.unpin(targetNode.id);
		dragSimulation = null;
		store.setPosition(targetNode.id, targetNode.position.x, targetNode.position.y);
	};

	const LEGEND_OUTCOMES: ReadonlyArray<{ label: string; outcome: string }> = [
		{ label: 'Success', outcome: 'success' },
		{ label: 'Partial', outcome: 'partial' },
		{ label: 'Failure', outcome: 'failure' },
		{ label: 'Unjudged', outcome: 'unknown' }
	];
	const similarities = $derived(visibleEdges.map((e) => e.similarity));
	const minSimilarity = $derived(similarities.length > 0 ? Math.min(...similarities) : null);
	const maxSimilarity = $derived(similarities.length > 0 ? Math.max(...similarities) : null);
</script>

<div
	class="eg-wrap"
	class:eg-wrap--compact={mode === 'compact'}
	class:eg-wrap--full={mode === 'full'}
	data-slot="embedding-graph"
>
	{#if mode === 'compact' && expandHref}
		<Tooltip text="Open full graph" placement="left">
			{#snippet child({ props })}
				<a
					{...props}
					href={expandHref}
					class="eg-corner-btn"
					aria-label="Open full graph"
					data-augmented-ui="tl-clip border"
				>
					<UnfoldHorizontal size={13} aria-hidden="true" />
				</a>
			{/snippet}
		</Tooltip>
	{/if}
	{#if mode === 'full' && collapseHref}
		<Tooltip text="Back" placement="left">
			{#snippet child({ props })}
				<a
					{...props}
					href={collapseHref}
					class="eg-corner-btn"
					aria-label="Collapse graph"
					data-augmented-ui="tl-clip border"
				>
					<FoldHorizontal size={13} aria-hidden="true" />
				</a>
			{/snippet}
		</Tooltip>
	{/if}

	<div class="eg-canvas" bind:this={canvasEl}>
		<SvelteFlow
			bind:nodes={flowNodes}
			edges={flowEdges}
			{nodeTypes}
			fitView
			fitViewOptions={{ padding: 0.15, maxZoom: 1 }}
			minZoom={0.1}
			maxZoom={2}
			nodesConnectable={false}
			elementsSelectable={false}
			panOnScroll={mode === 'full'}
			zoomOnScroll
			proOptions={{ hideAttribution: true }}
			colorMode="system"
			{onnodedragstart}
			{onnodedrag}
			{onnodedragstop}
		>
			{#snippet children()}
				<FlowController onReady={(fit) => (doFit = fit)} />
				<Background variant={BackgroundVariant.Dots} gap={22} size={1} />
				{#if mode === 'full'}
					<Controls showLock={false} />
					<MiniMap pannable zoomable />
					<Panel position="top-left">
						<div
							class="eg-toolbar"
							data-slot="embedding-graph-toolbar"
							data-augmented-ui="tl-clip br-clip border"
						>
							<label class="eg-toolbar__threshold" for="eg-threshold-slider">
								<span>Min similarity</span>
								<Slider
									id="eg-threshold-slider"
									aria-label="Minimum similarity"
									value={store.similarityThreshold * 100}
									min={30}
									max={100}
									step={5}
									onValueChange={(v) => (store.similarityThreshold = v / 100)}
								/>
								<span class="eg-toolbar__value">{Math.round(store.similarityThreshold * 100)}%</span
								>
							</label>
							<button
								type="button"
								class="eg-toolbar__btn"
								onclick={() => void handleRelayout()}
								aria-label="Relayout"
								title="Relayout"
							>
								<RotateCcw size={12} aria-hidden="true" />
								Relayout
							</button>
							<button
								type="button"
								class="eg-toolbar__btn"
								onclick={() => doFit?.()}
								aria-label="Fit to view"
								title="Fit to view"
							>
								<Locate size={12} aria-hidden="true" />
								Fit
							</button>
						</div>
					</Panel>
				{/if}
			{/snippet}
		</SvelteFlow>
	</div>

	{#if mode === 'compact'}
		<div class="eg-caption" data-slot="embedding-graph-caption">
			{#if selected}
				<span class="eg-caption-title">{selected.title}</span>
				<button
					type="button"
					class="eg-caption-open"
					data-augmented-ui="tl-clip border"
					onclick={() => onOpenSession(selected.id)}
				>
					Open
				</button>
			{:else}
				<span class="eg-caption-hint">click a session to inspect it</span>
			{/if}
		</div>
	{/if}

	{#if mode === 'full'}
		<div class="eg-details funk-sec edge" data-accent="muted" data-slot="embedding-graph-details">
			<div class="funk-sec-hd">
				<span class="funk-sec-tag-backing">
					<span class="funk-sec-tag" data-augmented-ui="tr-clip bl-clip border">Details</span>
				</span>
			</div>
			<div class="funk-sec-body eg-details__body">
				{#if selected}
					<h3 class="eg-details__title">{selected.title}</h3>
					<dl class="eg-details__grid">
						{#if selected.projectPath}
							<dt>Project</dt>
							<dd>{selected.projectPath}</dd>
						{/if}
						<dt>Outcome</dt>
						<dd>{graphOutcomeLabel(selected.outcome)}</dd>
						<dt>Last seen</dt>
						<dd title={formatClockAbsolute(selected.lastSeen)}>
							{formatRelative(selected.lastSeen)}
						</dd>
						{#if !selected.isCentre}
							<dt>Similarity</dt>
							<dd>{Math.round(selected.similarity * 100)}%</dd>
						{/if}
					</dl>
					{#if selected.topics.length > 0}
						<div class="eg-details__topics">
							{#each selected.topics as topic (topic)}
								<span class="eg-details__topic-chip">{topic}</span>
							{/each}
						</div>
					{/if}
					<div class="eg-details__actions">
						<button
							type="button"
							class="eg-details__btn"
							onclick={() => onOpenSession(selected.id)}
						>
							Open session
						</button>
						{#if !selected.isCentre}
							{#if selected.expanded}
								<button
									type="button"
									class="eg-details__btn"
									onclick={() => store.collapse(selected.id)}
								>
									Collapse
								</button>
							{:else}
								<button
									type="button"
									class="eg-details__btn"
									onclick={() => void handleExpand(selected.id)}
								>
									Expand neighbours
								</button>
							{/if}
						{/if}
					</div>
				{:else}
					<p class="eg-details__hint">Select a session to see its details.</p>
				{/if}
			</div>
		</div>

		<div class="eg-legend" data-slot="embedding-graph-legend">
			{#each LEGEND_OUTCOMES as item (item.outcome)}
				<span class="eg-legend__item">
					<span
						class="eg-legend__swatch"
						style:background={`var(${graphOutcomeColorVar(item.outcome)})`}
					></span>
					{item.label}
				</span>
			{/each}
			{#if minSimilarity !== null && maxSimilarity !== null}
				<span class="eg-legend__range"
					>Similarity {Math.round(minSimilarity * 100)}%–{Math.round(maxSimilarity * 100)}%</span
				>
			{/if}
		</div>
	{/if}

	{#if previewNode && previewOriginEl}
		<SessionPreviewModal
			node={previewNode}
			originEl={previewOriginEl}
			onClose={closePreview}
			{onOpenSession}
			{fetchSession}
		/>
	{/if}
</div>

<style>
	.eg-wrap {
		--xy-background-color: var(--surface-bg);
		--xy-background-pattern-color: var(--border-subtle);
		--xy-node-background-color: var(--surface-card);
		--xy-node-border: 1px solid var(--border-subtle);
		--xy-node-color: var(--text-1);
		--xy-node-border-radius: var(--r-tol);
		--xy-node-boxshadow-hover: none;
		--xy-node-boxshadow-selected: none;
		--xy-edge-stroke: var(--border-default);
		--xy-edge-stroke-selected: var(--funk-cyan);
		--xy-edge-stroke-width: 1.5;
		--xy-connectionline-stroke: var(--funk-cyan);
		--xy-connectionline-stroke-width: 1.5;
		--xy-handle-background-color: transparent;
		--xy-handle-border-color: transparent;
		--xy-selection-background-color: var(--funk-cyan-dim);
		--xy-selection-border: 1px solid var(--funk-cyan-border);
		--xy-controls-button-background-color: var(--surface-card);
		--xy-controls-button-background-color-hover: var(--surface-2);
		--xy-controls-button-color: var(--text-2);
		--xy-controls-button-color-hover: var(--text-strong);
		--xy-controls-button-border-color: var(--border-subtle);
		--xy-controls-box-shadow: var(--shadow-sm);
		--xy-minimap-background-color: var(--surface-card);
		--xy-minimap-mask-background-color: var(--funk-cyan-dim);
		--xy-minimap-mask-stroke-color: var(--funk-cyan-border);
		--xy-minimap-mask-stroke-width: 1;
		--xy-minimap-node-background-color: var(--surface-3);
		--xy-minimap-node-stroke-color: var(--border-default);
		--xy-attribution-background-color: transparent;

		position: relative;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.eg-wrap--compact {
		height: 240px;
	}
	.eg-wrap--full {
		flex: 1 1 auto;
		min-height: 460px;
		display: grid;
		grid-template-columns: 1fr 260px;
		grid-template-rows: 1fr auto;
		gap: var(--space-3, 12px);
	}

	.eg-canvas {
		flex: 1 1 auto;
		position: relative;
		min-height: 0;
		border-radius: var(--r-tol);
		overflow: hidden;
	}
	.eg-wrap--full .eg-canvas {
		grid-column: 1;
		grid-row: 1;
	}

	.eg-corner-btn {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		position: absolute;
		top: 6px;
		right: 6px;
		z-index: 5;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		background: var(--surface-card);
		color: var(--text-2);
		text-decoration: none;
		transition:
			color var(--dur-1, 120ms) ease,
			border-color var(--dur-1, 120ms) ease;
	}
	.eg-corner-btn:hover {
		--aug-border-bg: var(--border-default);
		color: var(--text-strong);
	}
	.eg-corner-btn:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}

	.eg-toolbar {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2, 8px) var(--space-3, 12px);
		max-width: min(300px, calc(100cqw - 20px));
		padding: 6px 10px;
		background: var(--surface-card);
		font-size: 11px;
		color: var(--text-2);
	}
	.eg-toolbar__threshold {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.eg-toolbar__threshold :global(.sl-root) {
		width: 90px;
	}
	.eg-toolbar__value {
		font-family: var(--font-mono, monospace);
		min-width: 2.5em;
	}
	.eg-toolbar__btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 3px 7px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-tol);
		color: var(--text-2);
		font-size: 10px;
		cursor: pointer;
	}
	.eg-toolbar__btn:hover {
		color: var(--text-strong);
		border-color: var(--border-default);
	}
	.eg-toolbar__btn:focus-visible {
		outline: 2px solid var(--focus);
	}

	.eg-caption {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2, 8px);
		padding-top: var(--space-2, 8px);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		font-family: var(--font-mono, monospace);
	}
	.eg-caption-title {
		overflow-wrap: anywhere;
	}
	.eg-caption-open {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		flex-shrink: 0;
		padding: 2px 8px;
		background: var(--surface-2);
		color: var(--status-info-fg);
		font-size: 10px;
		cursor: pointer;
	}
	.eg-caption-open:hover {
		--aug-border-bg: var(--border-default);
	}
	.eg-caption-open:focus-visible {
		outline: 2px solid var(--focus);
	}
	.eg-caption-hint {
		font-style: italic;
	}

	.eg-details {
		grid-column: 2;
		grid-row: 1 / span 2;
		min-width: 0;
	}
	.eg-details__body {
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
	}
	.eg-details__title {
		margin: 0 0 var(--space-2, 8px);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 13px;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.eg-details__grid {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 4px 8px;
		margin: 0 0 var(--space-3, 12px);
		font-size: 11px;
	}
	.eg-details__grid dt {
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		font-size: 9px;
	}
	.eg-details__grid dd {
		margin: 0;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.eg-details__topics {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		margin-bottom: var(--space-3, 12px);
	}
	.eg-details__topic-chip {
		padding: 1px 6px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
		color: var(--text-2);
		font-size: 10px;
		font-family: var(--font-mono, monospace);
	}
	.eg-details__actions {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.eg-details__btn {
		padding: 5px 8px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-tol);
		color: var(--text-1);
		font-size: 11px;
		cursor: pointer;
		text-align: left;
	}
	.eg-details__btn:hover {
		border-color: var(--border-default);
		color: var(--text-strong);
	}
	.eg-details__btn:focus-visible {
		outline: 2px solid var(--focus);
	}
	.eg-details__hint {
		margin: 0;
		font-size: 11px;
		color: var(--text-3);
		font-style: italic;
	}

	.eg-legend {
		grid-column: 1;
		grid-row: 2;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px;
		padding: 8px 4px 0;
		font-size: 10px;
		color: var(--text-3);
		border-top: 1px solid var(--border-subtle);
	}
	.eg-legend__item {
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}
	.eg-legend__swatch {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		flex-shrink: 0;
	}
	.eg-legend__range {
		margin-left: auto;
		font-family: var(--font-mono, monospace);
	}

	@container embedding-graph-full (max-width: 640px) {
		.eg-wrap--full {
			grid-template-columns: 1fr;
			grid-template-rows: minmax(320px, 1fr) auto auto;
		}
		.eg-wrap--full .eg-canvas {
			grid-column: 1;
			grid-row: 1;
		}
		.eg-details {
			grid-column: 1;
			grid-row: 2;
			max-height: 320px;
		}
		.eg-legend {
			grid-column: 1;
			grid-row: 3;
		}
	}

	:global(.svelte-flow__minimap) {
		box-shadow: var(--shadow-sm);
		border-radius: var(--r-tol);
	}
</style>
