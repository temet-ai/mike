<script lang="ts">
	interface Props {
		scopeLabel: string;
		shownCount?: number;
		totalCount?: number;
		itemNoun?: string;
		itemNounPlural?: string;
		dirty?: boolean;
		dirtyMessage?: string;
	}

	const {
		scopeLabel,
		shownCount,
		totalCount,
		itemNoun = 'item',
		itemNounPlural,
		dirty = false,
		dirtyMessage = 'Selection changed — press Apply to update'
	}: Props = $props();

	const showCount = $derived(shownCount !== undefined && totalCount !== undefined);
	const resolvedPlural = $derived(itemNounPlural ?? `${itemNoun}s`);
</script>

<div class="filter-scope-status" role="status" aria-live="polite" data-slot="filter-scope-status">
	<span class="filter-scope-status__scope" data-slot="filter-scope-status-scope">
		Scope: {scopeLabel}
		{#if showCount}
			· showing <span class="filter-scope-status__kpi">{shownCount!.toLocaleString()}</span> of
			<span class="filter-scope-status__kpi">{totalCount!.toLocaleString()}</span>
			{totalCount === 1 ? itemNoun : resolvedPlural}
		{/if}
	</span>
	{#if dirty}
		<span class="filter-scope-status__dirty" data-slot="filter-scope-status-dirty"
			>{dirtyMessage}</span
		>
	{/if}
</div>

<style>
	.filter-scope-status {
		flex-basis: 100%;
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 4px var(--space-3);
		padding-top: var(--space-2);
		margin-top: var(--space-1);
		border-top: var(--hairline) solid var(--border-subtle);
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.filter-scope-status__kpi {
		font-family: var(--font-display);
		font-weight: var(--weight-heavy);
		color: var(--text-strong);
	}
	.filter-scope-status__dirty {
		color: var(--status-warning-fg);
	}
</style>
