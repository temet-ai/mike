<script lang="ts">
	import type { Snippet } from 'svelte';
	import { ArrowUpRight, Star } from '@lucide/svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { pinnedProjectOrder } from '$lib/stores/pinnedProjectOrder.svelte';
	import { resolve } from '$app/paths';
	import { formatRelative } from '$lib/time';
	import CommsThreadFilters from './CommsThreadFilters.svelte';
	import CommsThreadRows from './CommsThreadRows.svelte';
	import { describeGroupCounts, groupThreadsByProject } from './groupThreads';
	import type { ResolvedFilter } from './commsStore.svelte';
	import type { ThreadSummaryDto, ThreadTagDto } from './types';

	interface Props {
		threads: ThreadSummaryDto[];
		tags: ThreadTagDto[];
		selectedThreadUuid: string | null;
		filterTag: string | null;
		filterParticipant: string | null;
		filterResolved: ResolvedFilter;
		now: number;
		loading: boolean;
		newThread?: Snippet;
		onSelect: (uuid: string) => void;
		onFilterTag: (tag: string | null) => void;
		onClearParticipant: () => void;
		onFilterResolved: (v: ResolvedFilter) => void;
	}

	const {
		threads,
		tags,
		selectedThreadUuid,
		filterTag,
		filterParticipant,
		filterResolved,
		now,
		loading,
		newThread,
		onSelect,
		onFilterTag,
		onClearParticipant,
		onFilterResolved
	}: Props = $props();

	const hasActiveFilter = $derived(
		filterTag !== null || filterParticipant !== null || filterResolved !== 'all'
	);

	const groups = $derived(groupThreadsByProject(threads, pinnedProjectOrder.order));
	let collapsedKeys = $state<Set<string>>(new Set());

	function toggleGroup(key: string): void {
		const next = new Set(collapsedKeys);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		collapsedKeys = next;
	}

	function clearFilters(): void {
		onFilterTag(null);
		onClearParticipant();
		onFilterResolved('all');
	}
</script>

<div class="thread-list funk-sec edge" data-accent="teal">
	<div class="thread-list__band">
		{#if newThread}
			<div class="thread-list__new">{@render newThread()}</div>
		{/if}
		<CommsThreadFilters
			{tags}
			{filterTag}
			{filterParticipant}
			{filterResolved}
			{onFilterTag}
			{onClearParticipant}
			{onFilterResolved}
		/>
	</div>

	<div class="funk-sec-body">
		{#if loading}
			<div class="thread-list__skeleton" aria-hidden="true">
				<div class="thread-list__skel-row"></div>
				<div class="thread-list__skel-row"></div>
				<div class="thread-list__skel-row thread-list__skel-row--short"></div>
			</div>
			<p class="sr-only" role="status">Loading threads…</p>
		{:else if threads.length === 0}
			{#if hasActiveFilter}
				<p class="thread-list__empty">
					No threads match the current filters.
					<button type="button" class="thread-list__clear-filters" onclick={clearFilters}
						>Clear filters</button
					>
				</p>
			{:else}
				<p class="thread-list__empty thread-list__empty--welcome">
					No threads yet. Threads are conversations exchanged with other sessions — start one above
					to see it here.
				</p>
			{/if}
		{:else}
			{#each groups as group (group.key)}
				{@const collapsed = collapsedKeys.has(group.key)}
				<section class="thread-group" aria-label={group.name} data-kind={group.kind}>
					<h2 class="thread-group__heading">
						<button
							type="button"
							class="thread-group__toggle"
							aria-expanded={!collapsed}
							aria-controls="thread-group-body-{group.key}"
							title={group.path ?? undefined}
							onclick={() => toggleGroup(group.key)}
						>
							<DisclosureMarker open={!collapsed} size={10} />
							{#if group.kind === 'pinned'}
								<Star class="thread-group__pin" size={11} fill="currentColor" aria-label="Pinned" />
							{/if}
							<span class="thread-group__name">{group.name}</span>
							{#if group.path}<span class="thread-group__path">{group.path}</span>{/if}
							<span class="thread-group__counts"
								>{describeGroupCounts(group)} · updated {formatRelative(
									group.lastMessageAt,
									now
								)}</span
							>
						</button>
						{#if group.path}
							<a
								class="thread-group__open"
								href={resolve(
									`/projects/${encodeURIComponent(group.path)}` as `/projects/${string}`
								)}
								aria-label={`Open project ${group.name}`}
								title="Open the project"><ArrowUpRight size={12} aria-hidden="true" /></a
							>
						{/if}
					</h2>
					{#if !collapsed}
						<CommsThreadRows
							threads={group.threads}
							{selectedThreadUuid}
							{now}
							{onSelect}
							id="thread-group-body-{group.key}"
						/>
					{/if}
				</section>
			{/each}
		{/if}
	</div>
</div>

<style>
	.thread-list {
		min-width: 0;
	}

	.thread-list__band {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}

	.thread-list__empty {
		margin: 0;
		padding-top: var(--space-3);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		color: var(--text-3);
	}

	.thread-list__empty--welcome {
		line-height: 1.6;
	}

	.thread-list__clear-filters {
		padding: 0;
		border: none;
		background: none;
		color: var(--status-info-fg);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}

	.thread-list__clear-filters:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.thread-list__skeleton {
		display: flex;
		flex-direction: column;
		gap: 0;
		padding-top: var(--space-3);
	}

	.thread-list__skel-row {
		height: 38px;
		border-bottom: var(--hairline) solid var(--border-subtle);
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
	}

	.thread-list__skel-row--short {
		height: 30px;
		opacity: 0.7;
	}

	.thread-list__skel-row {
		animation: thread-list-shimmer 1.6s ease-in-out infinite;
	}

	@keyframes thread-list-shimmer {
		0% {
			background-position: 200% 0;
		}
		100% {
			background-position: -200% 0;
		}
	}

	.thread-group {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.thread-group + .thread-group {
		border-top: var(--hairline) solid var(--border-default);
	}

	.thread-group__heading {
		display: flex;
		align-items: stretch;
		margin: 0;
	}

	.thread-group__open {
		display: inline-flex;
		align-items: center;
		padding: 0 var(--space-2);
		color: var(--text-3);
	}

	.thread-group__open:hover {
		color: var(--interactive-active-fg);
	}

	.thread-group__open:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	:global(.thread-group__pin) {
		align-self: center;
		color: var(--interactive-active-fg);
		fill: currentColor;
	}

	.thread-group__path {
		min-width: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}

	.thread-group__toggle {
		all: unset;
		box-sizing: border-box;
		display: flex;
		align-items: baseline;
		flex: 1;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-2);
		min-width: 0;
		padding: var(--space-2) 0;
		cursor: pointer;
	}

	.thread-group__toggle:hover {
		background: var(--surface-2);
	}

	.thread-group__toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.thread-group__name {
		font-family: var(--font-mono, monospace);
		font-weight: var(--weight-display);
		font-size: var(--text-md);
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}

	.thread-group__counts {
		margin-left: auto;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
</style>
