<script lang="ts">
	import { Dialog as DialogPrimitive } from 'bits-ui';
	import { Button } from '$lib/components/ui/button';
	import XIcon from '@lucide/svelte/icons/x';
	import { cn, type WithoutChildrenOrChild } from '$lib/utils';
	import type { ComponentProps, Snippet } from 'svelte';
	import SheetPortal from './sheet-portal.svelte';
	import SheetOverlay from './sheet-overlay.svelte';

	let {
		ref = $bindable(null),
		class: className,
		portalProps,
		children,
		showCloseButton = true,
		...restProps
	}: WithoutChildrenOrChild<DialogPrimitive.ContentProps> & {
		portalProps?: WithoutChildrenOrChild<ComponentProps<typeof SheetPortal>>;
		children: Snippet;
		showCloseButton?: boolean;
	} = $props();
</script>

<SheetPortal {...portalProps}>
	<SheetOverlay />
	<DialogPrimitive.Content
		bind:ref
		data-slot="sheet-content"
		data-augmented-ui="tl-clip bl-clip border"
		class={cn(
			'fixed inset-y-0 right-0 z-50 flex max-h-dvh w-full max-w-[420px] flex-col bg-[var(--surface-card)] text-[var(--text-1)] outline-none',
			className
		)}
		{...restProps}
	>
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			data-slot="sheet-body"
			class="min-h-0 flex-1 overflow-auto overscroll-contain focus-visible:outline-2 focus-visible:-outline-offset-4 focus-visible:outline-[var(--interactive-focus-ring)]"
			tabindex="0"
		>
			{@render children?.()}
		</div>
		{#if showCloseButton}
			<DialogPrimitive.Close data-slot="sheet-close">
				{#snippet child({ props })}
					<Button
						variant="ghost"
						size="icon"
						aria-label="Close"
						class="absolute top-3 right-3"
						{...props}
					>
						<XIcon class="size-4" aria-hidden="true" />
					</Button>
				{/snippet}
			</DialogPrimitive.Close>
		{/if}
	</DialogPrimitive.Content>
</SheetPortal>

<style>
	:global([data-slot='sheet-content'][data-augmented-ui]) {
		position: fixed !important;
		overflow-wrap: anywhere;
		--edge: var(--edge-md);
		--aug-tl: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-info-fg) 0%,
			var(--status-info-border) 35%,
			var(--border-default) 70%
		);
	}
	:global([data-slot='sheet-body']) {
		scrollbar-gutter: stable;
		padding-bottom: env(safe-area-inset-bottom, 0px);
	}
</style>
