<script lang="ts" generics="T extends string">
	import { mergeProps } from 'bits-ui';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';

	interface Choice {
		value: T;
		label: string;
		description?: string;
	}

	interface Props {
		label: string;
		choices: Choice[];
		value: T | null;
		onChange: (value: T) => void;
	}

	let { label, choices, value, onChange }: Props = $props();
</script>

<div class="ecc" role="radiogroup" aria-label={label}>
	<span class="ecc-label machine">{label}</span>
	<div class="ecc-chips">
		{#each choices as choice (choice.value)}
			{#if choice.description}
				<Tooltip text={choice.description}>
					{#snippet child({ props })}
						{@render chip(choice, props)}
					{/snippet}
				</Tooltip>
			{:else}
				{@render chip(choice, {})}
			{/if}
		{/each}
	</div>
</div>

{#snippet chip(choice: Choice, triggerProps: Record<string, unknown>)}
	<button
		{...mergeProps(triggerProps, { onclick: () => onChange(choice.value) })}
		type="button"
		role="radio"
		class="ecc-chip edge machine"
		aria-checked={choice.value === value}><span>{choice.label}</span></button
	>
{/snippet}

<style>
	.ecc {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1);
	}
	.ecc-chips {
		display: flex;
		flex: 1 1 12rem;
		flex-wrap: wrap;
		gap: var(--space-1);
	}
	.ecc-label {
		font-size: var(--text-xs);
		color: var(--text-3);
		min-width: var(--ecc-label-w, 5.5rem);
	}
	.ecc-chip {
		--edge: 4px;
		--edge-pad: 0px;
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-2);
		padding: 2px 7px;
		font-size: var(--text-xs);
		color: var(--text-2);
		cursor: pointer;
	}
	.ecc-chip:hover {
		color: var(--text-strong);
	}
	.ecc-chip[aria-checked='true'] {
		--edge-line: var(--interactive-active-border);
		--edge-surface: var(--interactive-active-bg);
		color: var(--interactive-active-fg);
	}
	.ecc-chip:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}
</style>
