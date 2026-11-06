<script lang="ts">
	import type { Snippet } from 'svelte';

	export interface OverviewFact {
		label: string;
		value: string | number;
	}

	interface Props {
		label: string;
		facts: OverviewFact[];
		children?: Snippet;
	}

	const { label, facts, children }: Props = $props();
</script>

<div class="overview-band" data-slot="overview-band">
	<dl class="overview-facts" aria-label={label}>
		{#each facts as fact (fact.label)}
			<div>
				<dt>{fact.label}</dt>
				<dd>{typeof fact.value === 'number' ? fact.value.toLocaleString() : fact.value}</dd>
			</div>
		{/each}
	</dl>
	{#if children}
		<p class="overview-note">{@render children()}</p>
	{/if}
</div>

<style>
	.overview-band {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding: var(--space-4) var(--space-5);
		border-block: var(--hairline) solid var(--border-subtle);
		background: linear-gradient(100deg, var(--funk-cyan-dim), transparent 65%);
	}
	.overview-facts {
		display: flex;
		gap: var(--space-6);
		margin: 0;
	}
	.overview-facts > div {
		display: flex;
		flex-direction: column-reverse;
		gap: var(--space-1);
	}
	.overview-facts dt {
		color: var(--text-3);
		font-size: var(--text-xs);
	}
	.overview-facts dd {
		margin: 0;
		font: var(--weight-display) var(--text-xl) / var(--leading-tight) var(--font-display);
		font-variant-numeric: tabular-nums;
	}
	.overview-note {
		max-width: 50ch;
		margin: 0;
		color: var(--text-2);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
	}
</style>
