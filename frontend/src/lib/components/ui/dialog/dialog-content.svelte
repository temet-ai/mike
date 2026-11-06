<script lang="ts">
	import { Dialog as DialogPrimitive } from 'bits-ui';
	import DialogPortal from './dialog-portal.svelte';
	import type { Snippet } from 'svelte';
	import * as Dialog from './index';
	import { cn, type WithoutChildrenOrChild } from '$lib/utils';
	import type { ComponentProps } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import XIcon from '@lucide/svelte/icons/x';

	let {
		ref = $bindable(null),
		class: className,
		portalProps,
		children,
		showCloseButton = true,
		scrollBody = true,
		bodyClass,
		...restProps
	}: WithoutChildrenOrChild<DialogPrimitive.ContentProps> & {
		portalProps?: WithoutChildrenOrChild<ComponentProps<typeof DialogPortal>>;
		children: Snippet;
		showCloseButton?: boolean;
		scrollBody?: boolean;
		bodyClass?: string;
	} = $props();
</script>

<DialogPortal {...portalProps}>
	<Dialog.Overlay />
	<DialogPrimitive.Content
		bind:ref
		data-slot="dialog-content"
		data-has-close={showCloseButton ? '' : undefined}
		data-augmented-ui="tl-clip br-clip border"
		data-scroll-body={scrollBody ? '' : undefined}
		class={cn(
			'bg-card text-card-foreground max-h-[calc(100dvh-2*var(--space-4))] max-w-[calc(100%-2*var(--space-4))] text-sm sm:max-w-lg fixed top-1/2 left-1/2 z-50 w-full -translate-x-1/2 -translate-y-1/2 outline-none',
			scrollBody
				? 'flex flex-col overflow-hidden'
				: 'grid gap-4 overflow-auto overscroll-contain p-6',
			className
		)}
		{...restProps}
	>
		{#if scrollBody}
			<div
				data-slot="dialog-body"
				class={cn(
					'grid min-h-0 flex-1 gap-4 overflow-auto overscroll-contain p-6 [scrollbar-gutter:stable]',
					bodyClass
				)}
			>
				{@render children?.()}
			</div>
		{:else}
			{@render children?.()}
		{/if}
		{#if showCloseButton}
			<DialogPrimitive.Close data-slot="dialog-close">
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
</DialogPortal>

<style>
	:global([data-slot='dialog-content'][data-augmented-ui]) {
		position: fixed !important;
		overflow-wrap: anywhere;
		--edge: var(--edge-md);
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
	:global([data-slot='dialog-content'][data-scroll-body]) {
		scrollbar-gutter: auto;
	}
	:global([data-slot='dialog-content'][data-has-close] [data-slot='dialog-header']) {
		padding-inline-end: var(--control-h);
	}
</style>
