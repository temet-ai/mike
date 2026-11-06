<script lang="ts">
	import { resolve } from '$app/paths';
	import {
		groupBoardRows,
		matchesQuery,
		type BoardGroup,
		type CanvasEdge,
		type CanvasNode
	} from './graphNormalize';
	import FleetBoardRow from './FleetBoardRow.svelte';

	interface Props {
		nodes: CanvasNode[];
		historyCount: number;
		selectedId: string | null;
		searchQuery: string;
		now: number;
		onSelect: (id: string) => void;
		edges?: CanvasEdge[];
	}

	const {
		nodes,
		historyCount,
		selectedId,
		searchQuery,
		now,
		onSelect,
		edges = []
	}: Props = $props();

	const trimmedQuery = $derived(searchQuery.trim());
	const groups: BoardGroup[] = $derived(groupBoardRows(nodes, now));
	const connectedIds = $derived(
		new Set(edges.filter((e) => e.from !== e.to).flatMap((e) => [e.from, e.to]))
	);

	function isDimmed(node: CanvasNode): boolean {
		if (trimmedQuery.length > 0) return !matchesQuery(node, trimmedQuery);
		return selectedId !== null && node.id !== selectedId;
	}
</script>

<div class="flb" data-slot="fleet-board">
	{#each groups as group (group.kind)}
		<section class="flb-group" data-slot="fleet-board-group" data-group-kind={group.kind}>
			<h3 class="flb-group__title">
				{group.title}<span class="flb-group__count">{group.nodes.length}</span>
			</h3>
			<div class="flb-group__rows">
				{#each group.nodes as node (node.id)}
					<FleetBoardRow
						{node}
						selected={node.id === selectedId}
						dimmed={isDimmed(node)}
						hasComms={connectedIds.has(node.id)}
						{now}
						{onSelect}
					/>
				{/each}
			</div>
		</section>
	{/each}

	{#if historyCount > 0}
		<a class="flb-history-link" href={resolve('/sessions')} data-slot="fleet-board-history-link">
			{historyCount} other session{historyCount === 1 ? '' : 's'} in this window (no declared role, not
			connected, not recently active) — see all on /sessions
		</a>
	{/if}
</div>

<style>
	.flb {
		flex: 1;
		min-width: 0;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
	}
	@container fleet-workspace (max-width: 640px) {
		.flb {
			min-height: 0;
		}
	}
	.flb-group {
		border-bottom: 1px solid var(--border-subtle);
	}
	.flb-group__title {
		display: flex;
		align-items: baseline;
		gap: 8px;
		margin: 0;
		padding: 9px 14px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-label);
		font-weight: 600;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-3);
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
		position: sticky;
		top: 0;
		z-index: 1;
	}
	.flb-group__count {
		font-weight: 400;
		letter-spacing: normal;
		text-transform: none;
		color: var(--text-faint);
	}
	.flb-group__rows {
		display: flex;
		flex-direction: column;
	}
	.flb-history-link {
		display: block;
		padding: 12px 14px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		color: var(--text-3);
		text-decoration: none;
		transition: color 0.14s;
	}
	.flb-history-link:hover {
		color: var(--text-strong);
	}
	.flb-history-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}
</style>
