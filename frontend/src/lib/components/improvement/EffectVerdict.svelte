<!--
@component
The honest read of one action's effect: "not enough data yet" below the read point, or a
before/after share with its confidence interval once it clears it. Never a bare percentage — see
design-spec.md §4.10 and ASK.md's ruled-out list ("An effect verdict that prints a share
difference without the interval and the read point").

Takes `MeasuredEffectResult`, not the full `EffectResult` union — a guard question's `randomised`
effect is a different measure (holdback comparison, not before/after) and is never handed to this
component; `ActionEffectRow` renders that branch itself (ui-designer ruling, design-spec.md
§4.8/§4.10). The narrower prop type makes that enforced by the compiler, not just by convention.
-->
<script lang="ts">
	import type { MeasuredEffectResult } from '$lib/api/improvement';

	interface Props {
		effect: MeasuredEffectResult;
	}

	const { effect }: Props = $props();
</script>

{#if effect.status === 'not_computed'}
	<p class="ev ev--neutral" data-slot="effect-verdict" data-status="not_computed">
		Effect not computed yet. No measurement is available.
	</p>
{:else if effect.status === 'not_enough_data'}
	<p class="ev ev--neutral" data-slot="effect-verdict" data-status="not_enough_data">
		Not enough data yet ({effect.n} of {effect.readPoint} turns judged so far).
	</p>
{:else if effect.verdict === 'fewer_corrections'}
	<p class="ev ev--success" data-slot="effect-verdict" data-status="fewer_corrections">
		Fewer corrections: {effect.before.sharePct}% of {effect.before.n} turns in the {effect.before
			.windowDays} days before, {effect.after.sharePct}% of {effect.after.n} turns since — both past the
		{effect.readPoint}-turn read point this needed. The gap is unlikely to be chance ({effect.ciLowPct}–{effect.ciHighPct}
		percentage points).
	</p>
{:else}
	<p class="ev ev--neutral" data-slot="effect-verdict" data-status="no_change_seen">
		No change we can see: {effect.before.sharePct}% before, {effect.after.sharePct}% since ({effect
			.before.n} and {effect.after.n} turns, both past the {effect.readPoint}-turn read point this
		needed). The gap could be chance ({effect.ciLowPct}–{effect.ciHighPct} percentage points).
	</p>
{/if}

<style>
	.ev {
		margin: 0;
		font-size: var(--text-sm);
		line-height: var(--leading-normal, 1.5);
	}
	.ev--neutral {
		color: var(--text-1);
	}
	.ev--success {
		color: var(--status-success-fg);
	}
</style>
