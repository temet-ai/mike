<script lang="ts">
	import { ArrowLeft, BookOpen, RefreshCw } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';

	interface Props {
		title?: string;
		detail?: string;
		hint?: string;
		onRetry?: () => void;
		backHref?: string;
		backLabel?: string;
		docsHref?: string;
		docsLabel?: string;
		message?: string;
	}

	const {
		title,
		detail,
		hint,
		onRetry,
		backHref,
		backLabel = 'Back',
		docsHref,
		docsLabel = 'Open docs',
		message
	}: Props = $props();

	const resolvedTitle = $derived(title ?? 'Failed to load.');
	const resolvedDetail = $derived(detail ?? message ?? '');
	const hasActions = $derived(Boolean(onRetry) || Boolean(backHref) || Boolean(docsHref));
</script>

<div
	class="load-error-banner flex min-w-0 flex-col gap-3 bg-[var(--status-danger-bg)] text-sm text-[var(--status-danger-fg)]"
	data-augmented-ui="tl-clip br-clip border"
	role="alert"
	data-slot="load-error-banner"
>
	<div class="flex flex-col gap-1">
		<strong class="font-semibold text-[var(--status-danger-fg)]" data-slot="load-error-title">
			{resolvedTitle}
		</strong>
		{#if resolvedDetail}
			<p class="text-xs text-[var(--status-danger-fg)]" data-slot="load-error-detail">
				{resolvedDetail}
			</p>
		{/if}
		{#if hint}
			<p class="text-xs text-[var(--text-2)]" data-slot="load-error-hint">
				{hint}
			</p>
		{/if}
	</div>
	{#if hasActions}
		<div class="flex flex-wrap items-center gap-2" data-slot="load-error-actions">
			{#if onRetry}
				<Button variant="outline" onclick={onRetry} data-slot="load-error-retry">
					<RefreshCw class="size-3" aria-hidden="true" />
					<span>Retry</span>
				</Button>
			{/if}
			{#if backHref}
				<Button variant="outline" href={backHref} data-slot="load-error-back">
					<ArrowLeft class="size-3" aria-hidden="true" />
					<span>{backLabel}</span>
				</Button>
			{/if}
			{#if docsHref}
				<Button
					variant="outline"
					href={docsHref}
					target={docsHref.startsWith('http') ? '_blank' : undefined}
					rel={docsHref.startsWith('http') ? 'noopener noreferrer' : undefined}
					data-slot="load-error-docs"
				>
					<BookOpen class="size-3" aria-hidden="true" />
					<span>{docsLabel}</span>
				</Button>
			{/if}
		</div>
	{/if}
</div>

<style>
	.load-error-banner {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--status-danger-border);
		padding: calc(var(--edge) + var(--space-2));
		overflow-wrap: anywhere;
	}
	.load-error-banner :global([data-slot='load-error-actions'] > *) {
		height: auto;
		min-height: var(--control-h);
		padding-block: var(--space-2);
		white-space: normal;
		text-align: start;
	}
</style>
