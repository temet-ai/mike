<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		description?: string;
		children?: Snippet;
		actions?: Snippet;
		learnMoreHref?: string;
		learnMoreLabel?: string;
		level?: 'h1' | 'h2';
	}

	const {
		title,
		description,
		children,
		actions,
		learnMoreHref,
		learnMoreLabel = 'Learn more →',
		level = 'h2'
	}: Props = $props();
</script>

{#snippet intro()}
	<header class="page-intro" data-slot="page-intro">
		<svelte:element
			this={level}
			class="pi-title text-[var(--text-strong)]"
			data-slot="page-intro-title"
		>
			{title}
		</svelte:element>
		{#if children}
			<p class="pi-lede" data-slot="page-intro-lede">
				{@render children()}
			</p>
		{:else if description}
			<p class="pi-lede" data-slot="page-intro-lede">
				{description}
			</p>
		{/if}
		{#if learnMoreHref}
			<!-- eslint-disable svelte/no-navigation-without-resolve -- learnMoreHref accepts any internal /help route (guide or glossary-hash) from a closed but varied set of callers; no single resolve() overload branch covers all shapes -->
			<a
				class="pi-learn-more"
				data-motion="hover"
				href={learnMoreHref}
				data-slot="page-intro-learn-more"
			>
				{learnMoreLabel}
			</a>
			<!-- eslint-enable svelte/no-navigation-without-resolve -->
		{/if}
	</header>
{/snippet}

{#if actions}
	<div class="page-intro-row" data-slot="page-intro-row">
		{@render intro()}
		<div class="page-intro-actions" data-slot="page-intro-actions">{@render actions()}</div>
	</div>
{:else}
	{@render intro()}
{/if}

<style>
	.page-intro-row {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-4);
	}
	.page-intro-actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
	.page-intro {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.pi-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		font-weight: var(--weight-display);
		line-height: var(--leading-tight);
		letter-spacing: var(--track-display);
		text-wrap: balance;
	}
	.pi-lede {
		margin: 0;
		max-width: 75ch;
		color: var(--text-2);
		font-size: var(--text-md);
		line-height: var(--leading-body);
		text-wrap: pretty;
	}
	.pi-lede :global(.flex) {
		flex-wrap: wrap;
	}
	.pi-learn-more {
		display: inline-flex;
		align-items: center;
		min-height: var(--control-h);
		color: var(--funk-cyan);
		font-size: var(--text-sm);
		text-underline-offset: var(--space-1);
	}
	.pi-learn-more:hover,
	.pi-learn-more:focus-visible {
		text-decoration: underline;
	}
	.pi-learn-more:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
