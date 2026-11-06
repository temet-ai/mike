<script lang="ts">
	import { Search, X } from '@lucide/svelte';
	import type { HTMLInputAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';

	type Props = Omit<HTMLInputAttributes, 'class' | 'value' | 'type'> & {
		ref?: HTMLInputElement | null;
		class?: string;
		value?: string;
		label: string;
		clearLabel?: string;
		onClear?: () => void;
	};

	let {
		ref = $bindable(null),
		value = $bindable(''),
		class: className,
		label,
		clearLabel = 'Clear search',
		onClear,
		...restProps
	}: Props = $props();

	function clear(): void {
		value = '';
		onClear?.();
		ref?.focus();
	}
</script>

<div
	class={cn('search-field', className)}
	data-augmented-ui="tl-clip border"
	data-slot="search-field"
	data-motion="hover"
>
	<Search class="search-field-icon" size={13} aria-hidden="true" />
	<input
		bind:this={ref}
		bind:value
		type="search"
		class="search-field-input"
		autocomplete="off"
		autocapitalize="off"
		spellcheck="false"
		aria-label={label}
		{...restProps}
	/>
	<button
		type="button"
		class="search-field-clear"
		data-motion="show"
		aria-label={clearLabel}
		data-shown={value.length > 0 ? '' : undefined}
		tabindex={value.length > 0 ? 0 : -1}
		onclick={clear}
	>
		<X size={12} aria-hidden="true" />
	</button>
</div>

<style>
	.search-field {
		position: relative;
		display: flex;
		align-items: center;
		height: var(--control-h-lg);
		max-width: 100%;
		background: var(--surface-2);
		--aug-tl: var(--edge-sm);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--border-subtle);
		filter: drop-shadow(0 0 0 transparent) drop-shadow(0 0 0 transparent);
	}
	.search-field:hover {
		--aug-border-bg: var(--border-strong);
	}
	.search-field:focus-within {
		--aug-border-bg: var(--interactive-focus-ring);
		filter: drop-shadow(0 0 2px var(--interactive-focus-ring))
			drop-shadow(0 0 12px var(--interactive-focus-ring));
	}
	.search-field :global(.search-field-icon) {
		position: absolute;
		left: var(--space-2);
		color: var(--text-faint);
		pointer-events: none;
	}
	.search-field-input {
		width: min(21rem, 100%);
		min-width: 0;
		height: 100%;
		padding: 0 var(--space-6) 0 var(--space-6);
		border: none;
		border-radius: 0;
		outline: none;
		background: transparent;
		color: var(--text-1);
		font-family: var(--font-mono);
		font-size: var(--text-label);
	}
	.search-field-input::placeholder {
		color: var(--text-faint);
	}
	.search-field-input::-webkit-search-cancel-button,
	.search-field-input::-webkit-search-decoration {
		-webkit-appearance: none;
	}
	.search-field-clear {
		position: absolute;
		right: var(--space-2);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		padding: 0;
		border: none;
		border-radius: 2px;
		background: transparent;
		color: var(--text-faint);
		cursor: pointer;
		visibility: hidden;
		opacity: 0;
	}
	.search-field-clear[data-shown] {
		visibility: visible;
		opacity: 1;
	}
	.search-field-clear:hover {
		background: var(--surface-3);
		color: var(--text-strong);
	}
	.search-field-clear:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}
</style>
