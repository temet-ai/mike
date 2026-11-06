<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLInputAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';

	type CheckboxProps = Omit<HTMLInputAttributes, 'class' | 'children' | 'type' | 'checked'> & {
		ref?: HTMLInputElement | null;
		class?: string;
		checked?: boolean;
		label: string | Snippet;
	};

	let {
		ref = $bindable(null),
		checked = $bindable(false),
		class: className,
		label,
		disabled,
		...restProps
	}: CheckboxProps = $props();
</script>

<label class={cn('cbx', className)} data-slot="checkbox" data-disabled={disabled ? '' : undefined}>
	<input
		bind:this={ref}
		bind:checked
		class="cbx__input"
		type="checkbox"
		{disabled}
		{...restProps}
	/>
	<span class="cbx__box" aria-hidden="true" data-motion="select"></span>
	<span class="cbx__label">
		{#if typeof label === 'string'}{label}{:else}{@render label()}{/if}
	</span>
</label>

<style>
	.cbx {
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: var(--space-3);
		min-block-size: var(--space-5);
		color: var(--text-1);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.cbx[data-disabled] {
		cursor: not-allowed;
		opacity: 0.5;
	}

	.cbx__input {
		position: absolute;
		inline-size: 16px;
		block-size: 16px;
		margin: 0;
		opacity: 0;
		cursor: inherit;
	}

	.cbx__box {
		position: relative;
		flex: 0 0 auto;
		inline-size: 16px;
		block-size: 16px;
		border: var(--hairline) solid var(--border-strong);
		background: var(--surface-2);
	}

	.cbx__box::after {
		content: '';
		position: absolute;
		inset: 4px;
		background: var(--interactive-active-fg);
		opacity: 0;
	}

	.cbx__input:checked + .cbx__box {
		border-color: var(--interactive-active-border);
		background: var(--interactive-active-bg);
	}

	.cbx__input:checked + .cbx__box::after {
		opacity: 1;
	}

	.cbx:hover:not([data-disabled]) .cbx__box {
		border-color: var(--interactive-active-border);
	}

	.cbx__input:focus-visible + .cbx__box {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
