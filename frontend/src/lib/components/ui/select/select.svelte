<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLSelectAttributes } from 'svelte/elements';
	import { ChevronDown } from '@lucide/svelte';
	import { cn } from '$lib/utils';

	export interface SelectOption {
		value: string;
		label?: string;
		disabled?: boolean;
	}

	type SelectProps = Omit<HTMLSelectAttributes, 'class' | 'children' | 'value'> & {
		ref?: HTMLSelectElement | null;
		class?: string;
		value?: string;
		options: SelectOption[];
		label?: string | Snippet;
		hint?: string;
	};

	let {
		ref = $bindable(null),
		value = $bindable(''),
		class: className,
		options,
		label,
		hint,
		...restProps
	}: SelectProps = $props();
</script>

<label class={cn('sel', className)} data-slot="select">
	{#if label || hint}
		<span class="machine sel__label" data-slot="select-label">
			{#if typeof label === 'string'}{label}{:else if label}{@render label()}{/if}
			{#if hint}<span class="sel__hint">{hint}</span>{/if}
		</span>
	{/if}
	<span class="edge sel__frame">
		<select bind:this={ref} bind:value {...restProps}>
			{#each options as option (option.value)}
				<option value={option.value} disabled={option.disabled}>
					{option.label ?? option.value}
				</option>
			{/each}
		</select>
		<ChevronDown class="sel__caret" size={12} aria-hidden="true" />
	</span>
</label>

<style>
	.sel {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-inline-size: 0;
	}

	.sel__label {
		color: var(--text-2);
		font-size: var(--text-xs);
	}

	.sel:focus-within .sel__label {
		color: var(--interactive-focus-ring);
	}

	.sel__hint {
		color: var(--text-3);
		font-family: var(--font-mono);
		letter-spacing: normal;
		text-transform: none;
	}

	.sel__frame {
		--edge: var(--edge-sm);
		--edge-pad: 0px;
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-2);
		display: flex;
		align-items: center;
		flex: 0 0 auto;
		block-size: var(--control-h);
	}

	.sel__frame:focus-within {
		--edge-line: var(--interactive-focus-ring);
	}

	.sel__frame > select {
		flex: 1 1 0;
		min-inline-size: 0;
		block-size: 100%;
		padding-inline: calc(var(--edge-sm) + var(--space-1)) var(--space-6);
		border: 0;
		outline: none;
		appearance: none;
		background: transparent;
		color: var(--text-1);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.sel__frame > select:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}

	.sel__frame > select > option {
		background: var(--surface-2);
		color: var(--text-1);
	}

	.sel__frame > :global(.sel__caret) {
		position: absolute;
		inset-inline-end: var(--space-3);
		color: var(--text-3);
		pointer-events: none;
	}
</style>
