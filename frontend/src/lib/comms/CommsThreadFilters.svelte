<script lang="ts">
	import { Users } from '@lucide/svelte';
	import { shortSessionId } from '$lib/format';
	import { filterableTags } from './groupThreads';
	import type { ResolvedFilter } from './commsStore.svelte';
	import type { ThreadTagDto } from './types';

	interface Props {
		tags: ThreadTagDto[];
		filterTag: string | null;
		filterParticipant: string | null;
		filterResolved: ResolvedFilter;
		onFilterTag: (tag: string | null) => void;
		onClearParticipant: () => void;
		onFilterResolved: (v: ResolvedFilter) => void;
	}

	const {
		tags,
		filterTag,
		filterParticipant,
		filterResolved,
		onFilterTag,
		onClearParticipant,
		onFilterResolved
	}: Props = $props();
</script>

<div class="thread-filters">
	<label class="thread-filters__field">
		<span class="thread-filters__label">Tag</span>
		<select
			class="thread-filters__select"
			value={filterTag ?? ''}
			onchange={(e) => onFilterTag(e.currentTarget.value || null)}
		>
			<option value="">All tags</option>
			{#each filterableTags(tags, filterTag) as tag (tag.code)}
				<option value={tag.code}>{tag.code} ({tag.usage_count})</option>
			{/each}
		</select>
	</label>

	<div class="thread-filters__resolved" role="group" aria-label="Filter by resolved state">
		{#each [['all', 'All'], ['open', 'Open'], ['resolved', 'Resolved']] as [value, label] (value)}
			<button
				type="button"
				class="thread-filters__seg"
				class:thread-filters__seg--active={filterResolved === value}
				aria-pressed={filterResolved === value}
				onclick={() => onFilterResolved(value as ResolvedFilter)}
			>
				{label}
			</button>
		{/each}
	</div>

	<div class="thread-filters__chips">
		{#if filterParticipant}
			<button type="button" class="thread-filters__participant" onclick={onClearParticipant}>
				<Users size={11} aria-hidden="true" />
				{shortSessionId(filterParticipant)}
				<span aria-hidden="true">×</span>
				<span class="sr-only">Clear participant filter</span>
			</button>
		{/if}
	</div>
</div>

<style>
	.thread-filters {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.thread-filters__field {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.thread-filters__label {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		text-transform: uppercase;
		color: var(--text-3);
	}

	.thread-filters__select {
		padding: 3px 6px;
		border: var(--hairline) solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
	}

	.thread-filters__resolved {
		display: inline-flex;
		border: var(--hairline) solid var(--border-subtle);
		border-radius: 2px;
		overflow: hidden;
	}

	.thread-filters__seg {
		padding: 3px var(--space-2);
		border: none;
		border-right: var(--hairline) solid var(--border-subtle);
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		cursor: pointer;
	}

	.thread-filters__seg:last-child {
		border-right: none;
	}

	.thread-filters__seg--active {
		background: var(--interactive-active-bg, var(--surface-card));
		color: var(--text-strong);
	}

	.thread-filters__chips {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		margin-left: auto;
	}

	.thread-filters__participant {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		padding: 3px var(--space-2);
		border: var(--hairline) solid var(--interactive-active-border);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-strong);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		cursor: pointer;
	}
</style>
