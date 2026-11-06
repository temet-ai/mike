<script lang="ts">
	import { Popover as PopoverPrimitive } from 'bits-ui';
	import { cn, type WithoutChildrenOrChild } from '$lib/utils';
	import type { ComponentProps, Snippet } from 'svelte';
	import PopoverPortal from './popover-portal.svelte';

	let {
		ref = $bindable(null),
		class: className,
		portalProps,
		sideOffset = 6,
		collisionPadding = 8,
		align = 'start',
		children,
		...restProps
	}: WithoutChildrenOrChild<PopoverPrimitive.ContentProps> & {
		portalProps?: WithoutChildrenOrChild<ComponentProps<typeof PopoverPortal>>;
		children: Snippet;
	} = $props();
</script>

<PopoverPortal {...portalProps}>
	<PopoverPrimitive.Content
		bind:ref
		data-slot="popover-content"
		{sideOffset}
		{collisionPadding}
		{align}
		data-augmented-ui="tl-clip br-clip border"
		class={cn(
			'z-50 max-h-[var(--bits-floating-available-height)] w-80 max-w-[calc(100dvw-2*var(--space-2))] overflow-auto overscroll-contain bg-[var(--surface-card)] p-3 text-[var(--text-1)] outline-none',
			className
		)}
		{...restProps}
	>
		{@render children?.()}
	</PopoverPrimitive.Content>
</PopoverPortal>

<style>
	:global([data-slot='popover-content'][data-augmented-ui]) {
		transform-origin: var(--bits-floating-transform-origin);
		overflow-wrap: anywhere;
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-info-fg) 0%,
			var(--status-info-border) 35%,
			var(--border-default) 70%
		);
	}
</style>
