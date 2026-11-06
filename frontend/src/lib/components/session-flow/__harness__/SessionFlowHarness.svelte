<script lang="ts">
	import { onDestroy } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { createFlowPlayback } from '$lib/session-flow/playback.svelte';
	import { sceneAt } from '$lib/session-flow/projection';
	import { fleetDataset, peakMs } from '$lib/session-flow/__fixtures__/fleet';
	import { flowTime } from '$lib/session-flow/__fixtures__/sessionFlow';
	import type { SessionFlowAvailability } from '$lib/api/generated/SessionFlowAvailability';
	import type { FlowBounds, FlowSelection } from '$lib/session-flow/types';
	import type { FlowLiveMark } from '$lib/session-flow/sessionFlowStore.svelte';
	import SessionFlow from '../SessionFlow.svelte';

	const dataset = fleetDataset();
	const playback = createFlowPlayback({ dataset });
	playback.seek(peakMs);

	let selection = $state<FlowSelection>(null);
	let followId = $state<string | null>(null);
	let collapsedIds = $state<ReadonlySet<string>>(new Set());
	let brush = $state<FlowBounds | null>(null);
	let overlay = $state<readonly FlowLiveMark[]>([]);
	let action = $state('No payload opened');
	let loading = $state(false);

	const availability: SessionFlowAvailability = {
		eligible: { capture_version: 1, admitted_at: flowTime(0), producer: 'claude_code' }
	};
	const scene = $derived(sceneAt(dataset, playback.atMs ?? peakMs));

	onDestroy(() => playback.dispose());
</script>

<p class="p-4" data-testid="flow-harness-action">{action}</p>
<button
	type="button"
	class="p-4"
	onclick={() => {
		loading = !loading;
	}}
>
	Toggle loading state
</button>
<SessionFlow
	{dataset}
	{scene}
	{playback}
	{selection}
	{availability}
	{loading}
	{followId}
	{collapsedIds}
	{brush}
	{overlay}
	onSelect={(next) => {
		selection = next;
	}}
	onSeek={(at, options) => {
		playback.seek(at, options);
	}}
	onFollow={(id) => {
		followId = id;
	}}
	onCollapse={(id) => {
		const next = new SvelteSet(collapsedIds);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		collapsedIds = next;
	}}
	onBrush={(range) => {
		brush = range;
	}}
	onOpenDrawer={(id, kind) => {
		action = `Open ${kind} payload ${id}`;
	}}
/>
