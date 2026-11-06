<!--
@component
Everything the operator has told Claude three or more times lately, as one framed list with a
hairline between groups. Each group shows its count, when it happened, the weekly bars and every
message in full. Bars share one scale across the groups so they compare between rows. A failed
load shows the load error with a retry; no groups shows one line saying so.
-->
<script lang="ts">
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import type { RepeatsResponse } from '$lib/api/improvement';
	import RepeatedAskGroup from './RepeatedAskGroup.svelte';
	import { scaleMax } from './repeatedAsks';

	interface Props {
		repeats: RepeatsResponse | null;
		error: string | null;
		onRetry: () => void;
		now?: number;
	}

	const { repeats, error, onRetry, now = Date.now() }: Props = $props();

	const groups = $derived(repeats?.groups ?? []);
	const max = $derived(scaleMax(groups));
</script>

{#if error}
	<LoadErrorBanner title="Could not load what you keep saying." detail={error} {onRetry} />
{:else if groups.length === 0}
	<p class="ra-empty">
		Nothing you have said three times in the last {repeats?.weeks ?? 12} weeks.
	</p>
{:else if repeats}
	<p class="ra-scope">
		From {repeats.turns} of your messages the judge labelled Corrected or Redirected in the last {repeats.weeks}
		weeks.
	</p>
	<div class="ra-list edge mech-group" data-slot="repeated-asks">
		{#each groups as group (group.id)}
			<RepeatedAskGroup {group} weekStarts={repeats.week_starts} scaleMax={max} {now} />
		{/each}
	</div>
{/if}

<style>
	.ra-empty,
	.ra-scope {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.ra-list {
		--edge: var(--edge-sm);
		min-width: 0;
	}
</style>
