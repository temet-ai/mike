<script lang="ts">
	import { Tooltip as TooltipPrimitive } from 'bits-ui';
	import { cn, type WithoutChildrenOrChild } from '$lib/utils';
	import type { ComponentProps, Snippet } from 'svelte';
	import TooltipPortal from './tooltip-portal.svelte';

	let {
		ref = $bindable(null),
		class: className,
		portalProps,
		sideOffset = 6,
		collisionPadding = 8,
		side = 'top',
		align = 'center',
		children,
		...restProps
	}: WithoutChildrenOrChild<TooltipPrimitive.ContentProps> & {
		portalProps?: WithoutChildrenOrChild<ComponentProps<typeof TooltipPortal>>;
		children: Snippet;
	} = $props();
</script>

<TooltipPortal {...portalProps}>
	<TooltipPrimitive.Content
		bind:ref
		data-slot="tooltip-content"
		{sideOffset}
		{collisionPadding}
		{side}
		{align}
		data-augmented-ui="tl-clip br-clip border"
		class={cn(
			'z-50 max-h-[var(--bits-floating-available-height)] max-w-[min(280px,calc(100dvw-2*var(--space-2)))] overflow-auto overscroll-contain bg-[var(--surface-card)] px-3 py-2 text-xs leading-snug text-[var(--text-1)] outline-none',
			className
		)}
		{...restProps}
	>
		{@render children?.()}
	</TooltipPrimitive.Content>
</TooltipPortal>

<style>
	:global([data-slot='tooltip-content'][data-augmented-ui]) {
		overflow-wrap: anywhere;
		--edge: 5px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-strong);
	}
</style>
