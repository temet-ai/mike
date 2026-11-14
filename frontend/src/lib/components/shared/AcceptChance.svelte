<script lang="ts">
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { chanceText } from '$lib/suggestedOrder';

	interface Props {
		chance: number | null;
		show?: boolean;
	}

	const { chance, show = true }: Props = $props();

	const TIP =
		"Jev's guess at the chance you accept this. It only orders the list; nothing is accepted for you.";
</script>

{#if show}
	<Tooltip text={TIP}>
		{#snippet child({ props })}
			<span
				{...props}
				class="ac"
				data-slot="accept-chance"
				data-known={chance === null ? undefined : ''}
			>
				accept {chanceText(chance)}
			</span>
		{/snippet}
	</Tooltip>
{/if}

<style>
	.ac {
		display: inline-block;
		min-width: 11ch;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
		text-align: right;
		white-space: nowrap;
		color: var(--text-3);
		cursor: help;
	}
	.ac[data-known] {
		color: var(--text-2);
	}
</style>
