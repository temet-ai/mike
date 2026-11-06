<script lang="ts" module>
	export type ColumnState = 'quiet' | 'busy' | 'error' | 'recent';
	export type ColumnControl = 'collapse' | 'expand' | 'none';
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Minus, Plus } from '@lucide/svelte';
	import { cn } from '$lib/utils';

	interface Props {
		label: string;
		count?: number;
		summary?: string;
		state?: ColumnState;
		control?: ColumnControl;
		onToggle?: () => void;
		height: number;
		children?: Snippet;
	}

	let {
		label,
		count,
		summary,
		state = 'quiet',
		control = 'none',
		onToggle,
		height,
		children
	}: Props = $props();

	const SUMMARY_COLOR: Readonly<Record<ColumnState, string>> = {
		quiet: 'var(--text-2)',
		busy: 'var(--signal-active)',
		error: 'var(--signal-error)',
		recent: 'var(--signal-complete)'
	};

	const summaryColor = $derived(SUMMARY_COLOR[state]);
	const expanded = $derived(control === 'collapse');
</script>

<div class="board-column" style="height: {height}px;">
	<div class="board-column__caption machine">
		<span class="board-column__label">{label}</span>
		{#if count !== undefined}
			<span class="board-column__count">{count}</span>
		{/if}
		{#if summary}
			<span class="board-column__state" style="color: {summaryColor};">{summary}</span>
		{/if}
		{#if control !== 'none'}
			<button
				type="button"
				class={cn('board-column__control', 'edge')}
				style="--edge: 4px; --edge-pad: 0px; --edge-line: var(--border-default); --edge-surface: var(--surface-2);"
				aria-expanded={expanded}
				aria-label={expanded ? `Collapse ${label}` : `Expand ${label}`}
				onclick={() => onToggle?.()}
			>
				{#if control === 'collapse'}
					<Minus size={12} aria-hidden="true" />
				{:else}
					<Plus size={12} aria-hidden="true" />
				{/if}
			</button>
		{/if}
	</div>
	{@render children?.()}
</div>

<style>
	.board-column {
		position: relative;
		width: 100%;
		background: var(--surface-scaffold);
		border-left: 1px solid var(--border-default);
		border-right: 1px solid var(--border-default);
	}
	.board-column__caption {
		position: absolute;
		top: 22px;
		left: 16px;
		right: 16px;
		display: flex;
		align-items: center;
		gap: var(--space-2, 8px);
		font-size: 9px;
		color: var(--text-2);
	}
	.board-column__count {
		color: var(--text-3);
	}
	.board-column__state {
		white-space: nowrap;
	}
	.board-column__control {
		margin-left: auto;
		flex: 0 0 auto;
		width: 20px;
		height: 20px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border: 0;
		padding: 0;
		color: var(--text-2);
		cursor: pointer;
	}
</style>
