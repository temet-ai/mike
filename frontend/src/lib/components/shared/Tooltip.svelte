<script lang="ts">
	import * as TooltipUI from '$lib/components/ui/tooltip';
	import { ArrowUpRight } from '@lucide/svelte';
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import { helpFor } from '$lib/help/helpContent';

	type Placement = 'top' | 'bottom' | 'left' | 'right';

	interface ChildSnippetArgs {
		props: Record<string, unknown>;
	}

	interface BaseProps {
		placement?: Placement;
		delay?: number;
		disabled?: boolean;
		children?: Snippet;
		child?: Snippet<[ChildSnippetArgs]>;
	}

	type Props = BaseProps &
		(
			| {
					text: string;
					slug?: string;
			  }
			| {
					text?: string;
					slug: string;
			  }
		);

	const {
		text,
		slug,
		placement = 'top',
		delay = 120,
		disabled = false,
		children,
		child
	}: Props = $props();

	const hint = $derived(text ?? (slug ? (helpFor(slug)?.short ?? '') : ''));
	const learnMoreHref = $derived(slug ? `/help/glossary#${slug}` : undefined);
</script>

<TooltipUI.Root delayDuration={delay} {disabled}>
	{#if child}
		{@const customChild = child}
		<TooltipUI.Trigger>
			{#snippet child({ props }: ChildSnippetArgs)}
				{@render customChild({ props })}
			{/snippet}
		</TooltipUI.Trigger>
	{:else if learnMoreHref}
		<TooltipUI.Trigger>
			{#snippet child({ props }: ChildSnippetArgs)}
				<a
					{...props}
					class="tooltip-link inline-flex items-center text-inherit no-underline outline-none focus-visible:underline"
					href={resolve(learnMoreHref as `/help/glossary#${string}`)}
					target="_blank"
					rel="noopener"
					data-slot="tooltip-trigger-link"
				>
					{@render children?.()}
				</a>
			{/snippet}
		</TooltipUI.Trigger>
	{:else}
		<TooltipUI.Trigger class="tooltip-trigger inline-flex items-center" aria-label={hint}>
			{@render children?.()}
		</TooltipUI.Trigger>
	{/if}
	<TooltipUI.Content side={placement}>
		<span data-slot="tooltip-text">{hint}</span>
		{#if slug}
			<span
				class="ml-1 inline-flex items-center text-[var(--interactive-active-fg)] opacity-75"
				data-slot="tooltip-learn-more-cue"
				aria-hidden="true"
			>
				<ArrowUpRight size={11} />
			</span>
		{/if}
	</TooltipUI.Content>
</TooltipUI.Root>

<style>
	:global(.tooltip-trigger) {
		cursor: inherit;
		background: transparent;
		border: 0;
		padding: 0;
		font: inherit;
		color: inherit;
	}
	:global(.tooltip-trigger:focus-visible) {
		outline: none;
		box-shadow: 0 0 0 1px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	:global(.tooltip-link:focus-visible) {
		outline: none;
		box-shadow: 0 0 0 1px var(--interactive-focus-ring);
		border-radius: 2px;
	}
</style>
