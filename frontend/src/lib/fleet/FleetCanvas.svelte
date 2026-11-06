<script lang="ts">
	import { untrack } from 'svelte';
	import {
		buildChildrenMap,
		layeredLayout,
		ROW_PITCH,
		routeEdges,
		subtreeIds
	} from './canvasLayout';
	import FleetCanvasNode from './FleetCanvasNode.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import {
		groupBidirectionalEdges,
		matchesQuery,
		type CanvasEdge,
		type CanvasNode,
		type EdgeGroup
	} from './graphNormalize';

	interface Props {
		nodes: CanvasNode[];
		edges: CanvasEdge[];
		selectedId: string | null;
		searchQuery: string;
		centerRequest: number;
		now: number;
		scopeKey: string;
		onSelect: (id: string | null) => void;
	}

	const { nodes, edges, selectedId, searchQuery, centerRequest, now, scopeKey, onSelect }: Props =
		$props();

	const layout = $derived(layeredLayout(nodes, edges));
	const nodesById = $derived(new Map(nodes.map((n) => [n.id, n])));
	const childrenMap = $derived(buildChildrenMap(layout.parent));
	const trimmedQuery = $derived(searchQuery.trim());

	let expandedIds = $state<Set<string>>(new Set());
	$effect(() => {
		void scopeKey;
		untrack(() => {
			expandedIds = new Set();
		});
	});

	const visibleIds = $derived.by(() => {
		const visible = new Set<string>();
		for (const n of nodes) {
			if ((layout.levels.get(n.id) ?? 0) <= 1) visible.add(n.id);
		}
		let changed = true;
		while (changed) {
			changed = false;
			for (const n of nodes) {
				if (visible.has(n.id)) continue;
				const p = layout.parent.get(n.id);
				if (p !== undefined && visible.has(p) && expandedIds.has(p)) {
					visible.add(n.id);
					changed = true;
				}
			}
		}
		return visible;
	});

	$effect(() => {
		const id = selectedId;
		if (id === null || !layout.positions.has(id)) return;
		untrack(() => {
			if (visibleIds.has(id)) return;
			const chain: string[] = [];
			let cur = layout.parent.get(id);
			while (cur !== undefined) {
				chain.push(cur);
				cur = layout.parent.get(cur);
			}
			if (chain.length > 0) expandedIds = new Set([...expandedIds, ...chain]);
		});
	});

	interface BranchChip {
		id: string;
		parentId: string;
		x: number;
		y: number;
		count: number;
		errorCount: number;
		hiddenLabels: string[];
	}

	const chips = $derived.by((): BranchChip[] => {
		const out: BranchChip[] = [];
		for (const id of visibleIds) {
			const hidden = subtreeIds(childrenMap, id).filter((cid) => !visibleIds.has(cid));
			if (hidden.length === 0) continue;
			const pos = layout.positions.get(id);
			if (!pos) continue;
			const errorCount = hidden.filter((cid) => nodesById.get(cid)?.state === 'error').length;
			out.push({
				id: `chip:${id}`,
				parentId: id,
				x: pos.x,
				y: pos.y + ROW_PITCH,
				count: hidden.length,
				errorCount,
				hiddenLabels: hidden.map((cid) => nodesById.get(cid)?.label ?? cid)
			});
		}
		return out;
	});

	function expandBranch(parentId: string): void {
		expandedIds = new Set([...expandedIds, parentId]);
	}

	function isNodeDimmed(node: CanvasNode): boolean {
		if (trimmedQuery.length > 0) return !matchesQuery(node, trimmedQuery);
		return selectedId !== null && node.id !== selectedId;
	}

	let wrapEl: HTMLDivElement | undefined = $state();

	$effect(() => {
		void edges;
		void centerRequest;
		if (!wrapEl) return;
		untrack(() => {
			const currentLayout = layout;
			const focusId =
				selectedId !== null && currentLayout.positions.has(selectedId)
					? selectedId
					: currentLayout.focusNodeId;
			const pos = focusId !== null ? currentLayout.positions.get(focusId) : undefined;
			if (!pos || !wrapEl) return;
			wrapEl.scrollTo({
				left: Math.max(0, pos.x - wrapEl.clientWidth / 2),
				top: Math.max(0, pos.y - wrapEl.clientHeight / 2)
			});
		});
	});

	const visibleGroups = $derived(
		groupBidirectionalEdges(edges).filter((g) => visibleIds.has(g.from) && visibleIds.has(g.to))
	);
	const routed = $derived(routeEdges(visibleGroups, layout.positions, layout.levels));

	let hoveredEdgeId = $state<string | null>(null);

	function isEdgeHot(group: EdgeGroup): boolean {
		if (hoveredEdgeId === group.id) return true;
		return selectedId !== null && (group.from === selectedId || group.to === selectedId);
	}
	function isEdgeDim(group: EdgeGroup): boolean {
		if (hoveredEdgeId !== null) return hoveredEdgeId !== group.id;
		return selectedId !== null && !isEdgeHot(group);
	}

	function strokeWidthFor(count: number): number {
		return Math.min(6, Math.max(1.5, 1.5 + Math.sqrt(count) * 1.1));
	}

	function edgeTooltip(group: EdgeGroup, fromLabel: string, toLabel: string): string {
		if (group.bidirectional) {
			return `${fromLabel} → ${toLabel}: ${group.forwardCount} message${group.forwardCount === 1 ? '' : 's'} · ${toLabel} → ${fromLabel}: ${group.backwardCount} message${group.backwardCount === 1 ? '' : 's'}`;
		}
		return `${fromLabel} → ${toLabel} · ${group.forwardCount} message${group.forwardCount === 1 ? '' : 's'}`;
	}

	function onWrapClick(e: MouseEvent): void {
		const target = e.target as HTMLElement | null;
		if (!target?.closest('[data-slot="fleet-node"], [data-slot="fleet-branch-chip"]'))
			onSelect(null);
	}
</script>

<div
	class="fleet-canvas-wrap"
	data-slot="fleet-canvas-wrap"
	bind:this={wrapEl}
	onclick={onWrapClick}
	onkeydown={(e) => {
		if (e.key === 'Escape') onSelect(null);
	}}
	role="presentation"
>
	<div
		class="fleet-canvas"
		style:width="{layout.width}px"
		style:height="{layout.height}px"
		data-slot="fleet-canvas"
		aria-label="Fleet session graph"
	>
		{#each nodes as node (node.id)}
			{@const pos = layout.positions.get(node.id)}
			{#if pos && visibleIds.has(node.id)}
				<FleetCanvasNode
					{node}
					x={pos.x}
					y={pos.y}
					selected={node.id === selectedId}
					dimmed={isNodeDimmed(node)}
					{now}
					{onSelect}
				/>
			{/if}
		{/each}

		{#each chips as chip (chip.id)}
			<Tooltip text={`Hidden: ${chip.hiddenLabels.join(', ')}`}>
				{#snippet child({ props })}
					<button
						{...props}
						type="button"
						class="fleet-branch-chip"
						style:left="{chip.x}px"
						style:top="{chip.y}px"
						onclick={() => expandBranch(chip.parentId)}
						data-slot="fleet-branch-chip"
						data-parent-id={chip.parentId}
					>
						<span>+{chip.count} more</span>
						{#if chip.errorCount > 0}
							<span class="fleet-branch-chip__errors"
								>· {chip.errorCount} error{chip.errorCount === 1 ? '' : 's'}</span
							>
						{/if}
					</button>
				{/snippet}
			</Tooltip>
		{/each}

		<svg
			class="fleet-canvas__edges"
			viewBox="0 0 {layout.width} {layout.height}"
			aria-hidden="true"
		>
			<defs>
				<marker
					id="fleet-edge-arrow"
					markerWidth="8"
					markerHeight="8"
					refX="7"
					refY="4"
					orient="auto-start-reverse"
					markerUnits="userSpaceOnUse"
				>
					<path d="M0,0 L8,4 L0,8 Z" fill="var(--status-steering-fg)" />
				</marker>
			</defs>
			{#each visibleGroups as group (group.id)}
				{@const route = routed.get(group.id)}
				{#if route}
					{@const width =
						strokeWidthFor(group.forwardCount + group.backwardCount) + (isEdgeHot(group) ? 1 : 0)}
					{@const fromLabel = nodesById.get(group.from)?.label ?? group.from}
					{@const toLabel = nodesById.get(group.to)?.label ?? group.to}
					<path
						class="fleet-edge-hit"
						d={route.path}
						data-slot="fleet-edge-hit"
						role="presentation"
						onpointerenter={() => (hoveredEdgeId = group.id)}
						onpointerleave={() => (hoveredEdgeId = null)}
					/>
					<path
						class="fleet-edge"
						class:fleet-edge--hot={isEdgeHot(group)}
						class:fleet-edge--dim={isEdgeDim(group)}
						d={route.path}
						style:stroke-width="{width}px"
						marker-end="url(#fleet-edge-arrow)"
						marker-start={group.bidirectional ? 'url(#fleet-edge-arrow)' : undefined}
						data-slot="fleet-edge"
						data-edge-id={group.id}
					>
						<title>{edgeTooltip(group, fromLabel, toLabel)}</title>
					</path>
					{#if group.bidirectional}
						<text
							class="fleet-edge__label"
							x={route.labelX - 22}
							y={route.labelY - 6}
							text-anchor="middle">{group.backwardCount}</text
						>
						<text
							class="fleet-edge__label"
							x={route.labelX + 22}
							y={route.labelY - 6}
							text-anchor="middle">{group.forwardCount}</text
						>
					{:else}
						<text
							class="fleet-edge__label"
							x={route.labelX}
							y={route.labelY - 6}
							text-anchor="middle">{group.forwardCount}</text
						>
					{/if}
				{/if}
			{/each}
		</svg>
	</div>
</div>

<style>
	.fleet-canvas-wrap {
		position: relative;
		flex: 1;
		min-width: 0;
		overflow: auto;
		background-image: radial-gradient(var(--border-subtle) 1px, transparent 1px);
		background-size: 26px 26px;
		background-position: 13px 13px;
	}
	@container fleet-workspace (max-width: 640px) {
		.fleet-canvas-wrap {
			min-height: 0;
		}
	}
	.fleet-canvas {
		position: relative;
		margin: auto;
	}
	.fleet-canvas__edges {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		pointer-events: none;
		overflow: visible;
	}
	.fleet-edge {
		fill: none;
		stroke: var(--status-steering-fg);
		opacity: 0.7;
		pointer-events: none;
		transition:
			opacity 0.16s,
			stroke-width 0.16s;
	}
	.fleet-edge-hit {
		fill: none;
		stroke: transparent;
		stroke-width: 12;
		pointer-events: stroke;
	}
	.fleet-edge--hot {
		opacity: 1;
	}
	.fleet-edge--dim {
		opacity: 0.12;
	}
	.fleet-edge__label {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-micro);
		fill: var(--text-2);
		paint-order: stroke;
		stroke: var(--surface-bg);
		stroke-width: 3px;
		pointer-events: none;
	}

	.fleet-branch-chip {
		position: absolute;
		width: 100px;
		height: 28px;
		transform: translate(-50%, -50%);
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 4px;
		border-radius: 999px;
		background: var(--surface-2);
		border: 1px solid var(--border-default);
		color: var(--text-2);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-caption);
		cursor: pointer;
		transition: border-color 0.14s;
	}
	.fleet-branch-chip:hover {
		border-color: var(--interactive-active-border);
	}
	.fleet-branch-chip:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}
	.fleet-branch-chip__errors {
		color: var(--status-danger-fg);
	}
</style>
