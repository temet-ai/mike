<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import { badgeVariants, needsClip, type BadgeVariant } from './Badge.variants';

	interface Props extends WithElementRef<HTMLAttributes<HTMLElement>> {
		variant?: BadgeVariant;
		href?: string;
	}

	let {
		ref = $bindable(null),
		class: className,
		variant = 'tag-default',
		href,
		children,
		...restProps
	}: Props = $props();

	const clip = $derived(needsClip(variant));
	const classes = $derived(cn(badgeVariants({ variant }), className));
</script>

{#if href}
	<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- href is caller-computed (ClassChip forwards routes/prompts/new's `boundIdentityHref`, typed `string | undefined` there); narrowing this prop to a resolve()-compatible literal type would require changing that out-of-scope caller and its stories file too. -->
	<a
		bind:this={ref}
		{href}
		data-slot="badge"
		data-augmented-ui={clip ? 'tl-clip br-clip border' : undefined}
		class={classes}
		{...restProps}
	>
		{@render children?.()}
	</a>
{:else}
	<span
		bind:this={ref}
		data-slot="badge"
		data-augmented-ui={clip ? 'tl-clip br-clip border' : undefined}
		class={classes}
		{...restProps}
	>
		{@render children?.()}
	</span>
{/if}

<style>
	:global(.badge-nx[data-augmented-ui]) {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		border: none;
		border-radius: 0;
	}
	:global(.status-pill[data-augmented-ui]) {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		border: none;
		border-radius: 0;
		box-shadow: none;
		padding: 2px 8px;
	}
	:global(.status-pill-complete[data-augmented-ui]) {
		--aug-border-bg: var(--status-success-border);
	}
	:global(.status-pill-streaming[data-augmented-ui]) {
		--aug-border-bg: var(--status-warning-border);
	}
	:global(.status-pill-pending[data-augmented-ui]) {
		--aug-border-bg: var(--amber-border-strong, var(--status-warning-fg));
	}
	:global(.status-pill-error[data-augmented-ui]) {
		--aug-border-bg: var(--status-danger-border);
	}
	:global(.status-pill-unknown[data-augmented-ui]) {
		--aug-border-bg: var(--border-default);
	}
	:global(.badge-active[data-augmented-ui]) {
		--aug-border-bg: var(--status-success-border);
	}
	:global(.badge-analyzed[data-augmented-ui]) {
		--aug-border-bg: var(--status-info-border);
	}
	:global(.badge-warning[data-augmented-ui]) {
		--aug-border-bg: var(--status-warning-border);
	}
	:global(.badge-error[data-augmented-ui]) {
		--aug-border-bg: var(--status-danger-border);
	}
	:global(.badge-muted[data-augmented-ui]) {
		--aug-border-bg: var(--status-muted-border);
	}

	:global(.class-chip) {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 6px;
		font-size: 0.6875rem;
		font-family: 'JetBrains Mono Variable', ui-monospace, monospace;
		color: var(--text-2);
		background: var(--surface-2);
		border: 1px solid var(--border-default);
		border-radius: 4px;
		text-decoration: none;
		transition:
			border-color 150ms ease,
			color 150ms ease;
	}
	:global(a.class-chip:hover) {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}
	:global(.class-chip-active) {
		border-left-color: var(--interactive-active-border);
		box-shadow: inset 2px 0 0 0 var(--interactive-active-border);
	}
</style>
