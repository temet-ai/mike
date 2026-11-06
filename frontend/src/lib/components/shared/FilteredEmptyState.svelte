<script lang="ts">
	import type { Component, Snippet } from 'svelte';
	import { Button } from '$lib/components/ui/button';

	interface Props {
		icon: Component<{ size?: number | string; 'aria-hidden'?: boolean; class?: string }>;
		filtered: boolean;
		emptyText: string;
		emptySubtext?: string;
		filteredText: string;
		filteredSubtext?: string;
		onClearFilters?: () => void;
		clearLabel?: string;
		children?: Snippet;
	}

	const {
		icon: Icon,
		filtered,
		emptyText,
		emptySubtext,
		filteredText,
		filteredSubtext,
		onClearFilters,
		clearLabel = 'Clear filters',
		children
	}: Props = $props();

	const text = $derived(filtered ? filteredText : emptyText);
	const subtext = $derived(filtered ? filteredSubtext : emptySubtext);
</script>

<div class="filtered-empty-state" role="status" aria-live="polite" data-slot="filtered-empty-state">
	<Icon size={24} aria-hidden={true} class="filtered-empty-state__icon" />
	<span class="filtered-empty-state__text" data-slot="filtered-empty-state-text">{text}</span>
	{#if subtext}
		<span class="filtered-empty-state__sub" data-slot="filtered-empty-state-sub">{subtext}</span>
	{/if}
	{#if filtered && children}
		<div class="filtered-empty-state__extra" data-slot="filtered-empty-state-extra">
			{@render children()}
		</div>
	{/if}
	{#if filtered && onClearFilters}
		<Button variant="outline" onclick={onClearFilters} data-slot="filtered-empty-state-clear">
			{clearLabel}
		</Button>
	{/if}
</div>

<style>
	.filtered-empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-7) var(--space-5);
		color: var(--text-3);
		text-align: center;
	}
	.filtered-empty-state :global(.filtered-empty-state__icon) {
		opacity: 0.4;
	}
	.filtered-empty-state__text {
		font-size: var(--text-sm);
	}
	.filtered-empty-state__sub {
		font-size: var(--text-caption);
		color: var(--text-faint);
		max-width: 48ch;
	}
	.filtered-empty-state__extra {
		width: 100%;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
	}
</style>
