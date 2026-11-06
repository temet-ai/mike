<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props extends WithElementRef<HTMLAttributes<HTMLElement>> {
		open?: boolean;
		ariaLabel: string;
		onToggle: () => void;
		head: Snippet;
		children: Snippet;
	}

	let {
		ref = $bindable(null),
		class: className,
		open = false,
		ariaLabel,
		onToggle,
		head,
		children,
		...restProps
	}: Props = $props();

	function handleKey(event: KeyboardEvent): void {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onToggle();
		}
	}
</script>

<div
	bind:this={ref}
	data-slot="message-view-subgroup"
	class={cn('subgroup', className)}
	{...restProps}
>
	<button
		type="button"
		class="subgroup__head w-full"
		aria-expanded={open}
		aria-label={ariaLabel}
		onclick={onToggle}
		onkeydown={handleKey}
	>
		<DisclosureMarker {open} size={12} />
		{@render head()}
	</button>
	{#if open}
		<div class="subgroup__body">
			{@render children()}
		</div>
	{/if}
</div>
