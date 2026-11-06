<!--
@component
One band of ActionEffectTable: what the action is (a lesson's rule, a directive's text, or a
guard question's own text), the project or agent it's scoped to, and its effect. `kind` never
renders as a bare word — the caller composes `label` to already say what it is. A lesson's effect
renders through the shared EffectVerdict (before/after share read); a guard question's effect is
a genuinely different measure (randomised holdback comparison, never before/after) and renders
its own sentence here, never through EffectVerdict, so the two methods' evidence quality is never
implied to be the same (ui-designer ruling, design-spec.md §4.8/§4.10, artifacts commit b75aff8).
-->
<script lang="ts">
	import EffectVerdict from './EffectVerdict.svelte';
	import type { ActionEffectItem } from '$lib/api/improvement';

	interface Props {
		action: ActionEffectItem;
	}

	const { action }: Props = $props();
</script>

<div class="band-body aer" data-slot="action-effect-row">
	<div class="band-hd-wrap aer-hd">
		<a class="aer-link" href={action.href}>{action.label}</a>
		<span class="band-hd__meta">{action.scope}</span>
	</div>
	{#if action.effect.status === 'randomised'}
		<p class="aer-randomised">
			Checked with a randomised comparison, not a before/after read: {action.effect.nHoldback} of {action
				.effect.readPoint} held-back turns collected so far ({action.effect.nTreated} delivered normally).
			See the full read on
			<a class="aer-link" href={action.effect.detailHref}>the guard page</a>.
		</p>
	{:else}
		<EffectVerdict effect={action.effect} />
	{/if}
</div>

<style>
	.aer {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.aer-hd {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
	}
	.aer-link {
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		color: var(--text-1);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.aer-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.aer-randomised {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
</style>
