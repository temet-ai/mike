<!--
@component
What every turn label means, and the rules the AI labellers followed for mixed cases, behind one
disclosure that is closed by default. Used on the Labelling page and on Review labels, so the label
definitions are readable without opening the judge's instructions.
-->
<script lang="ts">
	import type { VocabularyAxis } from '$lib/api/eval';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { AXES, LABELLING_RULES, ROW_LABEL } from './evalReview';
	import { optionsFor, wordFor } from './labelControl';

	let { vocabulary, open = false }: { vocabulary: VocabularyAxis[]; open?: boolean } = $props();

	// svelte-ignore state_referenced_locally
	let shown = $state(open);
</script>

<div class="ld" data-testid="label-definitions">
	<button type="button" class="ld-toggle" aria-expanded={shown} onclick={() => (shown = !shown)}>
		<DisclosureMarker open={shown} size={10} /><span
			>What each label means, and the rules the AI labellers followed</span
		>
	</button>
	{#if shown}
		<div class="ld-body">
			{#each AXES as axis (axis)}
				{@const options = optionsFor(vocabulary, axis)}
				<section class="ld-axis" aria-label={ROW_LABEL[axis]}>
					<h3 class="machine ld-k">{ROW_LABEL[axis]}</h3>
					{#if options.length > 0}
						<dl class="ld-list">
							{#each options as o (o.label)}
								<dt>{wordFor(o.label)}</dt>
								<dd>{o.description}</dd>
							{/each}
						</dl>
					{:else}
						<p class="ld-text">The definitions could not be loaded.</p>
					{/if}
					<p class="ld-text">For mixed cases:</p>
					<ul class="ld-rules">
						{#each LABELLING_RULES[axis] as rule (rule)}<li>{rule}</li>{/each}
					</ul>
				</section>
			{/each}
		</div>
	{/if}
</div>

<style>
	.ld {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.ld-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-sm);
		color: var(--text-2);
		text-align: left;
		cursor: pointer;
	}
	.ld-toggle:hover {
		color: var(--text-1);
	}
	.ld-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ld-body {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 320px), 1fr));
		gap: var(--space-4);
		max-width: 1200px;
	}
	.ld-axis {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.ld-k {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-2);
	}
	.ld-list {
		margin: 0;
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: var(--space-1) var(--space-3);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.ld-list dt {
		font-weight: var(--weight-display);
	}
	.ld-list dd {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.ld-text {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.ld-rules {
		margin: 0;
		padding-left: var(--space-4);
		list-style: square;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
</style>
