<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';
	import { getRadioGroupContext } from './context';

	type RadioGroupItemProps = Omit<HTMLAttributes<HTMLDivElement>, 'children'> & {
		ref?: HTMLDivElement | null;
		value: string;
		label: string | Snippet;
		description?: string;
		disabled?: boolean;
		children?: Snippet;
	};

	let {
		ref = $bindable(null),
		class: className,
		value,
		label,
		description,
		disabled = false,
		children,
		...restProps
	}: RadioGroupItemProps = $props();

	const group = getRadioGroupContext();
	const isDisabled = $derived(disabled || group.disabled);
</script>

<div
	bind:this={ref}
	data-slot="radio-group-item"
	data-motion="select"
	class={cn('rgi', className)}
	data-disabled={isDisabled ? '' : undefined}
	{...restProps}
>
	<label class="rgi__row">
		<input
			class="rgi__input"
			type="radio"
			name={group.name}
			{value}
			checked={group.value === value}
			disabled={isDisabled}
			onchange={() => group.select(value)}
		/>
		<span class="rgi__marker" aria-hidden="true" data-motion="select">
			<span class="rgi__dot"></span>
		</span>
		<span class="rgi__text">
			<span class="rgi__title">
				{#if typeof label === 'string'}{label}{:else}{@render label()}{/if}
			</span>
			{#if description}<span class="rgi__description">{description}</span>{/if}
		</span>
	</label>
	{#if children}
		<div class="rgi__extra">{@render children()}</div>
	{/if}
</div>

<style>
	.rgi {
		position: relative;
		min-inline-size: 0;
	}

	.rgi:hover:not([data-disabled]) {
		background: var(--surface-2);
	}

	.rgi:has(input:checked) {
		background: var(--interactive-active-bg);
	}

	.rgi:has(input:focus-visible) {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.rgi[data-disabled] {
		opacity: 0.5;
	}

	.rgi__row {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		cursor: pointer;
	}

	.rgi[data-disabled] .rgi__row {
		cursor: not-allowed;
	}

	.rgi__input {
		position: absolute;
		opacity: 0;
		inline-size: 1px;
		block-size: 1px;
	}

	.rgi__marker {
		flex: 0 0 auto;
		inline-size: 14px;
		block-size: 14px;
		margin-block-start: 2px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		background: var(--surface-2);
		border: 1px solid var(--border-strong);
		transform: rotate(45deg);
	}

	.rgi__dot {
		inline-size: 6px;
		block-size: 6px;
	}

	.rgi:has(input:checked) .rgi__marker {
		border-color: var(--funk-cyan-border);
	}

	.rgi:has(input:checked) .rgi__dot {
		background: var(--signal-active);
		box-shadow: var(--glow-cyan);
	}

	.rgi__text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1 1 auto;
		min-inline-size: 0;
	}

	.rgi__title {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.rgi:has(input:checked) .rgi__title {
		color: var(--interactive-active-fg);
	}

	.rgi__description {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}

	.rgi__extra {
		padding: 0 var(--space-3) var(--space-2) calc(var(--space-3) + 14px + var(--space-2));
	}
</style>
