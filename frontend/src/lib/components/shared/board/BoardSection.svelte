<script lang="ts">
	import { Minus, Plus } from '@lucide/svelte';
	import { cn } from '$lib/utils';
	import type { ColumnControl, ColumnState } from './BoardColumn.svelte';

	interface Props {
		label: string;
		count?: number;
		summary?: string;
		state?: ColumnState;
		control?: ColumnControl;
		onToggle?: () => void;
		y: number;
		divider?: boolean;
	}

	let {
		label,
		count,
		summary,
		state = 'quiet',
		control = 'none',
		onToggle,
		y,
		divider = true
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

<div class="board-section" style="top: {y - 10}px;">
	{#if divider}
		<div class="board-section__divider" aria-hidden="true"></div>
	{/if}
	<div class="board-section__head machine">
		<span class="board-section__label">{label}</span>
		{#if count !== undefined}
			<span class="board-section__count">{count}</span>
		{/if}
		{#if summary}
			<span class="board-section__state" style="color: {summaryColor};">{summary}</span>
		{/if}
		{#if control !== 'none'}
			<button
				type="button"
				class={cn('board-section__control', 'edge')}
				style="--edge: 4px; --edge-pad: 0px; --edge-line: var(--border-default); --edge-surface: var(--surface-2);"
				aria-expanded={expanded}
				aria-label={expanded ? `Collapse ${label}` : `Expand ${label}`}
				onclick={() => onToggle?.()}
			>
				{#if control === 'collapse'}
					<Minus size={10} aria-hidden="true" />
				{:else}
					<Plus size={10} aria-hidden="true" />
				{/if}
			</button>
		{/if}
	</div>
</div>

<style>
	.board-section {
		position: absolute;
		left: 16px;
		right: 16px;
	}
	.board-section__divider {
		border-top: 1px dashed var(--border-default);
		margin-bottom: 9px;
	}
	.board-section__head {
		display: flex;
		align-items: center;
		gap: var(--space-2, 8px);
		font-size: 8px;
		color: var(--text-3);
	}
	.board-section__count {
		color: var(--text-3);
	}
	.board-section__state {
		white-space: nowrap;
	}
	.board-section__control {
		margin-left: auto;
		flex: 0 0 auto;
		width: 16px;
		height: 16px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border: 0;
		padding: 0;
		color: var(--text-2);
		cursor: pointer;
	}
</style>
