<script module lang="ts">
	import { Handle, Position, type NodeProps } from '@xyflow/svelte';
	import { UnfoldHorizontal, ExternalLink } from '@lucide/svelte';
	import type { GraphNode } from '$lib/graph/embeddingGraphStore.svelte';
	import {
		graphOutcomeBorderVar,
		graphOutcomeColorVar,
		graphOutcomeLabel
	} from '$lib/components/graph/outcomeColors';

	export interface SessionFlowNodeData extends Record<string, unknown> {
		node: GraphNode;
		mode: 'compact' | 'full';
		isSelected: boolean;
		isDimmed: boolean;
		isIncident: boolean;
		onSelect: (id: string) => void;
		onExpand: (id: string) => void;
		onOpenPreview: (id: string, originEl: HTMLElement) => void;
		onHoverStart: (id: string) => void;
		onHoverEnd: (id: string) => void;
	}
</script>

<script lang="ts">
	let { data }: NodeProps & { data: SessionFlowNodeData } = $props();

	const node = $derived(data.node);
	const showChrome = $derived(data.mode === 'full');

	const borderRestVar = $derived(`var(${graphOutcomeBorderVar(node.outcome)})`);
	const borderHoverVar = $derived(`var(${graphOutcomeColorVar(node.outcome)})`);

	const badgeIsStrong = $derived(node.similarity >= 0.85);

	function ariaLabel(): string {
		if (node.isCentre) return `${node.title} — this session`;
		return `${node.title} — ${Math.round(node.similarity * 100)}% similar, select to open details`;
	}

	function onclick(e: MouseEvent): void {
		e.stopPropagation();
		data.onSelect(node.id);
	}

	function ondblclick(e: MouseEvent): void {
		e.stopPropagation();
		data.onExpand(node.id);
	}

	function onExpandClick(e: MouseEvent): void {
		e.stopPropagation();
		data.onExpand(node.id);
	}

	let sfnEl: HTMLElement | undefined;
	function onOpenPreviewClick(e: MouseEvent): void {
		e.stopPropagation();
		if (sfnEl) data.onOpenPreview(node.id, sfnEl);
	}

	function onmouseenter(): void {
		data.onHoverStart(node.id);
	}
	function onmouseleave(): void {
		data.onHoverEnd(node.id);
	}

	function onkeydown(e: KeyboardEvent): void {
		if (e.key === 'Enter' && e.shiftKey) {
			e.preventDefault();
			data.onExpand(node.id);
			return;
		}
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			data.onSelect(node.id);
		}
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -- purely a mouse-hover
     boundary around already-accessible children (`.sfn`'s own
     role="button"/tabindex/onkeydown, the expand button below); keyboard
     users reach the SAME highlight this hover drives by selecting the node,
     which `EmbeddingGraph.svelte` treats identically to a hover. -->
<div class="sfn-root" {onmouseenter} {onmouseleave}>
	<div class="sfn-handle-anchor" aria-hidden="true">
		<Handle type="target" id="c" position={Position.Top} isConnectable={false} />
		<Handle type="source" id="c" position={Position.Top} isConnectable={false} />
	</div>

	<div
		bind:this={sfnEl}
		class="sfn"
		class:sfn--centre={node.isCentre}
		class:eg-node--centre={node.isCentre}
		class:sfn--selected={data.isSelected && !node.isCentre}
		class:sfn--compact={data.mode === 'compact'}
		class:sfn--dimmed={data.isDimmed}
		class:sfn--incident={data.isIncident && !data.isSelected}
		role="button"
		tabindex="0"
		aria-label={ariaLabel()}
		aria-pressed={data.isSelected}
		data-slot="embedding-graph-node"
		data-session-id={node.id}
		data-augmented-ui="tl-clip br-clip border"
		style:--sfn-border-rest={borderRestVar}
		style:--sfn-border-hover={borderHoverVar}
		{onclick}
		{ondblclick}
		{onkeydown}
	>
		<span class="sfn-dot" style:background={`var(${graphOutcomeColorVar(node.outcome)})`}></span>
		<span class="sfn-title" title={node.title}>{node.title}</span>
		{#if !node.isCentre}
			<span class="sfn-badge" class:sfn-badge--strong={badgeIsStrong}
				>{Math.round(node.similarity * 100)}%</span
			>
		{/if}
		{#if node.expanded}
			<span class="sfn-expanded-mark" title="Neighbours expanded" aria-hidden="true"></span>
		{/if}

		{#if showChrome}
			{#if node.topics.length > 0}
				<div class="sfn-topics">
					{#each node.topics as topic (topic)}
						<span class="sfn-topic-chip">{topic}</span>
					{/each}
				</div>
			{/if}
			<span class="sfn-outcome">{graphOutcomeLabel(node.outcome)}</span>
		{/if}
	</div>

	{#if showChrome && !node.isCentre}
		<button
			type="button"
			class="sfn-expand-btn"
			aria-label={`Expand neighbours of ${node.title}`}
			title="Expand neighbours"
			onclick={onExpandClick}
		>
			<UnfoldHorizontal size={10} aria-hidden="true" />
		</button>
	{/if}

	{#if data.isSelected}
		<button
			type="button"
			class="sfn-open-btn"
			aria-label={`Open preview of ${node.title}`}
			title="Open preview"
			onclick={onOpenPreviewClick}
		>
			<ExternalLink size={10} aria-hidden="true" />
		</button>
	{/if}
</div>

<style>
	.sfn-root {
		position: relative;
		width: max-content;
	}

	.sfn-handle-anchor {
		position: absolute;
		top: 50%;
		left: 50%;
		width: 0;
		height: 0;
	}
	.sfn-handle-anchor :global(.svelte-flow__handle) {
		opacity: 0;
		pointer-events: none;
		width: 1px;
		height: 1px;
		min-width: 0;
		min-height: 0;
		border: 0;
		top: 0;
		left: 0;
		transform: none;
	}

	.sfn {
		--edge: 5px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--sfn-border-rest);
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 4px;
		width: max-content;
		max-width: 220px;
		padding: 5px 8px;
		background: var(--surface-card);
		cursor: pointer;
		outline-offset: 3px;
		transition:
			background-color var(--dur-1, 120ms) ease,
			opacity var(--dur-1, 120ms) ease,
			box-shadow var(--dur-2, 240ms) var(--ease-snap, cubic-bezier(0.2, 0.9, 0.1, 1));
	}
	.sfn:hover {
		--aug-border-bg: var(--sfn-border-hover);
		background: var(--surface-2);
	}
	.sfn:focus-visible {
		outline: 2px solid var(--focus);
	}
	.sfn--compact {
		--edge: 3px;
		padding: 3px 6px;
		max-width: 160px;
	}
	.sfn--dimmed {
		opacity: 0.5;
	}

	.sfn--incident {
		--aug-border-bg: var(--sfn-border-hover);
	}

	.sfn--selected {
		box-shadow:
			0 0 0 2px var(--surface-card),
			0 0 0 4px var(--focus);
	}

	.sfn--centre {
		--aug-border-bg: var(--funk-cyan-border);
		background: var(--funk-cyan-dim);
		box-shadow: var(--glow-cyan);
	}
	.sfn--centre:hover {
		--aug-border-bg: var(--funk-cyan-border);
		background: var(--funk-cyan-dim);
	}

	.sfn-dot {
		position: absolute;
		top: 5px;
		right: 6px;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.sfn-title {
		display: block;
		padding-right: 12px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 10px;
		line-height: 1.3;
		color: var(--text-1);
		white-space: normal;
		overflow-wrap: anywhere;
	}
	.sfn--centre .sfn-title {
		color: var(--funk-cyan);
		font-weight: 600;
	}

	.sfn-badge {
		align-self: flex-start;
		font-family: var(--font-mono, monospace);
		font-size: 9px;
		color: var(--text-3);
	}
	.sfn-badge--strong {
		color: var(--status-info-fg);
	}

	.sfn-expanded-mark {
		position: absolute;
		top: 5px;
		left: 6px;
		width: 4px;
		height: 4px;
		border-radius: 50%;
		background: var(--status-info-fg);
	}

	.sfn-topics {
		display: flex;
		flex-wrap: wrap;
		gap: 3px;
	}
	.sfn-topic-chip {
		padding: 1px 5px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
		color: var(--text-2);
		font-size: 9px;
		font-family: var(--font-mono, monospace);
	}

	.sfn-outcome {
		display: block;
		font-size: 9px;
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.sfn-expand-btn {
		position: absolute;
		top: -6px;
		right: -6px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		padding: 0;
		background: var(--surface-3);
		border: 1px solid var(--border-subtle);
		border-radius: 50%;
		color: var(--text-3);
		cursor: pointer;
	}
	.sfn-expand-btn:hover {
		color: var(--text-strong);
		border-color: var(--border-default);
	}
	.sfn-expand-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
	}

	.sfn-open-btn {
		position: absolute;
		bottom: -6px;
		right: -6px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		padding: 0;
		background: var(--surface-3);
		border: 1px solid var(--border-subtle);
		border-radius: 50%;
		color: var(--text-3);
		cursor: pointer;
	}
	.sfn-open-btn:hover {
		color: var(--text-strong);
		border-color: var(--border-default);
	}
	.sfn-open-btn:focus-visible {
		outline: 2px solid var(--focus);
	}
</style>
