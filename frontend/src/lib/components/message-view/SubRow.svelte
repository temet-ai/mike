<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import { Zap } from '@lucide/svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props extends WithElementRef<HTMLAttributes<HTMLElement>> {
		expandable?: boolean;
		open?: boolean;
		muted?: boolean;
		fired?: boolean;
		hideChevron?: boolean;
		onclick?: ((event: MouseEvent) => void) | undefined;
		onkeydown?: ((event: KeyboardEvent) => void) | undefined;
		ariaLabel?: string;
		leading?: Snippet;
		children: Snippet;
	}

	let {
		ref = $bindable(null),
		class: className,
		expandable = false,
		open = false,
		muted = false,
		fired = false,
		hideChevron = false,
		onclick,
		onkeydown,
		ariaLabel,
		leading,
		children,
		...restProps
	}: Props = $props();

	function handleKey(event: KeyboardEvent): void {
		if (!expandable) return;
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onclick?.(new MouseEvent('click'));
		}
		onkeydown?.(event);
	}
</script>

{#if expandable}
	<button
		type="button"
		bind:this={ref}
		data-slot="message-view-subrow"
		data-expandable="true"
		class={cn('subrow subrow--expandable w-full text-left', muted && 'subrow--muted', className)}
		aria-expanded={open}
		aria-label={ariaLabel}
		{onclick}
		onkeydown={handleKey}
		{...restProps}
	>
		{#if !hideChevron}
			<DisclosureMarker {open} size={12} />
		{/if}
		{#if fired}
			<Zap size={11} class="subrow__fired" aria-hidden="true" />
		{/if}
		{@render leading?.()}
		{@render children()}
	</button>
{:else}
	<div
		bind:this={ref}
		data-slot="message-view-subrow"
		class={cn('subrow', muted && 'subrow--muted', className)}
		{...restProps}
	>
		{#if !hideChevron}
			<span class="chev-spacer" aria-hidden="true"></span>
		{/if}
		{#if fired}
			<Zap size={11} class="subrow__fired" aria-hidden="true" />
		{/if}
		{@render leading?.()}
		{@render children()}
	</div>
{/if}

<style>
	:global(.subrow .chev-spacer) {
		flex: 0 0 12px;
		width: 12px;
		height: 12px;
	}
	:global(.subrow .subrow__fired) {
		flex: 0 0 auto;
		color: var(--status-success-fg);
	}
</style>
