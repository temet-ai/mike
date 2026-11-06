<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import SessionFlowBoard from '../SessionFlowBoard.svelte';
	import { fleetDataset, peakMs } from '$lib/session-flow/__fixtures__/fleet';
	import { defaultLegend } from '$lib/session-flow/legendEntries';
	import { sceneAt } from '$lib/session-flow/projection';
	import type { FlowSelection } from '$lib/session-flow/types';

	const dataset = fleetDataset();
	const scene = $derived(sceneAt(dataset, peakMs));

	let selection = $state<FlowSelection>(null);
	const legend = new SvelteSet<string>(defaultLegend());
	let collapsed = $state<boolean | undefined>(false);

	function dropToolcall(): void {
		legend.delete('toolcall');
	}
</script>

<button type="button" data-testid="harness-drop-toolcall" onclick={dropToolcall}
	>Drop toolcall from legend</button
>
<p data-testid="harness-selection">{JSON.stringify(selection)}</p>
<div data-design="funk" style="background: var(--surface-bg); overflow-x: auto;">
	<SessionFlowBoard
		{dataset}
		{scene}
		atMs={peakMs}
		{selection}
		onSelect={(next) => (selection = next)}
		{legend}
		bind:collapsed
	/>
</div>
