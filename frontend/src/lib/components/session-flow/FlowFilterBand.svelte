<script lang="ts">
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LegendFilter from '$lib/components/shared/LegendFilter.svelte';
	import { LEGEND_ENTRIES } from '$lib/session-flow/legendEntries';

	let {
		active = $bindable(),
		onChange
	}: {
		active: Set<string>;
		onChange?: (active: Set<string>) => void;
	} = $props();
</script>

<SectionCard title="Filter" bordered={false}>
	{#snippet headerActions()}
		<span class="flow-filter-band__meta">
			EVERY ENTRY IS A TOGGLE · DRAG ON THE TIMELINE TO SELECT A RANGE · SELECTING AN ITEM FOCUSES
			IT
		</span>
	{/snippet}
	<LegendFilter entries={LEGEND_ENTRIES} bind:active {onChange} />
</SectionCard>

<style>
	.flow-filter-band__meta {
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--text-3);
	}
</style>
