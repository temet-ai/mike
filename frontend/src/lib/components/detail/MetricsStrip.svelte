<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import type { RequestDetail } from '$lib/types';
	import { formatCount } from '$lib/time';
	import { cachePercent, formatCost } from '$lib/format';
	import { lookupModelPrice, priceNote } from '$lib/modelPricing';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { TOKEN_BAR_HELP } from '$lib/help/tokenBarHelp';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		detail: RequestDetail;
	}

	let { ref = $bindable(null), class: className, detail, ...restProps }: Props = $props();

	const usage = $derived(detail.token_usage);

	const cachePct = $derived(cachePercent(usage));

	const cachePctColor = $derived.by(() => {
		if (cachePct === null) return 'var(--text-2)';
		if (cachePct >= 80) return 'var(--status-success-fg)';
		if (cachePct >= 40) return 'var(--status-warning-fg)';
		return 'var(--text-2)';
	});

	const stopDisplay = $derived.by((): string => {
		const raw = detail.stop_reason;
		if (raw === null) return '—';
		switch (raw) {
			case 'end_turn':
				return 'end_turn';
			case 'max_tokens':
				return 'max_tokens';
			case 'stop_sequence':
				return 'stop_seq';
			case 'tool_use':
				return 'tool_use';
			default:
				return raw;
		}
	});

	const inputText = $derived(usage === null ? '—' : formatCount(usage.input_tokens));
	const outputText = $derived(usage === null ? '—' : formatCount(usage.output_tokens));
	const cacheReadText = $derived(usage === null ? '—' : formatCount(usage.cache_read_input_tokens));
	const cacheCreateText = $derived(
		usage === null ? '—' : formatCount(usage.cache_creation_input_tokens)
	);
	const cachePctText = $derived(cachePct === null ? '—' : `${cachePct}%`);
	const costText = $derived(formatCost(usage, detail.model));
	const costLookup = $derived(lookupModelPrice(detail.model));
	const costTip = $derived(priceNote(costLookup, detail.model));
</script>

<div
	bind:this={ref}
	data-slot="metrics-strip"
	class={cn(
		'flex h-9 items-stretch overflow-x-auto border-b border-[var(--border-default)] bg-[var(--surface-card)]',
		className
	)}
	{...restProps}
>
	<div
		data-slot="metric-cell"
		class="flex h-full min-w-[14ch] flex-none items-center border-r border-[var(--border-subtle)] px-3.5 last:border-r-0"
	>
		<span class="flex items-baseline gap-1.5">
			<Tooltip text={TOKEN_BAR_HELP.input.text} slug={TOKEN_BAR_HELP.input.slug}>
				<span class="text-micro uppercase tracking-kpi text-[var(--text-3)]">Input</span>
			</Tooltip>
			<span class="font-mono text-xs text-[var(--status-success-fg)]">{inputText}</span>
		</span>
	</div>
	<div
		data-slot="metric-cell"
		class="flex h-full min-w-[14ch] flex-none items-center border-r border-[var(--border-subtle)] px-3.5 last:border-r-0"
	>
		<span class="flex items-baseline gap-1.5">
			<Tooltip text={TOKEN_BAR_HELP.output.text} slug={TOKEN_BAR_HELP.output.slug}>
				<span class="text-micro uppercase tracking-kpi text-[var(--text-3)]">Output</span>
			</Tooltip>
			<span class="font-mono text-xs text-[var(--status-info-fg)]">{outputText}</span>
		</span>
	</div>
	<div
		data-slot="metric-cell"
		class="flex h-full min-w-[22ch] flex-none items-center border-r border-[var(--border-subtle)] px-3.5 last:border-r-0"
	>
		<span class="flex items-baseline gap-1.5">
			<Tooltip text={TOKEN_BAR_HELP.cacheRead.text} slug={TOKEN_BAR_HELP.cacheRead.slug}>
				<span class="text-micro uppercase tracking-kpi text-[var(--text-3)]">Cache read</span>
			</Tooltip>
			<span class="font-mono text-xs text-[var(--data-teal-fg)]">{cacheReadText}</span>
		</span>
	</div>
	<div
		data-slot="metric-cell"
		class="flex h-full min-w-[24ch] flex-none items-center border-r border-[var(--border-subtle)] px-3.5 last:border-r-0"
	>
		<span class="flex items-baseline gap-1.5">
			<Tooltip text={TOKEN_BAR_HELP.cacheCreate.text} slug={TOKEN_BAR_HELP.cacheCreate.slug}>
				<span class="text-micro uppercase tracking-kpi text-[var(--text-3)]">Cache create</span>
			</Tooltip>
			<span class="font-mono text-xs text-[var(--text-1)]">{cacheCreateText}</span>
		</span>
	</div>
	<div
		data-slot="metric-cell"
		class="flex h-full min-w-[13ch] flex-none items-center border-r border-[var(--border-subtle)] px-3.5 last:border-r-0"
	>
		<span class="flex items-baseline gap-1.5">
			<Tooltip text={TOKEN_BAR_HELP.cachePct.text} slug={TOKEN_BAR_HELP.cachePct.slug}>
				<span class="text-micro uppercase tracking-kpi text-[var(--text-3)]">Cache%</span>
			</Tooltip>
			<span class="font-mono text-xs text-[var(--kpi-color)]" style:--kpi-color={cachePctColor}
				>{cachePctText}</span
			>
		</span>
	</div>
	<div
		data-slot="metric-cell"
		class="flex h-full min-w-[15ch] flex-none items-center border-r border-[var(--border-subtle)] px-3.5 last:border-r-0"
	>
		<span class="flex items-baseline gap-1.5 whitespace-nowrap">
			<Tooltip text={costTip} slug={TOKEN_BAR_HELP.cost.slug}>
				<span class="text-micro uppercase tracking-kpi text-[var(--text-3)]">Cost</span>
			</Tooltip>
			<span class="font-mono text-xs text-[var(--status-warning-fg)]">{costText}</span>
			{#if !costLookup.known && costText !== '—'}
				<span data-slot="cost-estimate-note" class="text-micro text-[var(--text-3)]">estimate</span>
			{/if}
		</span>
	</div>
	<div
		data-slot="metric-cell"
		class="flex h-full min-w-[17ch] flex-none items-center border-r border-[var(--border-subtle)] px-3.5 last:border-r-0"
	>
		<span class="flex items-baseline gap-1.5">
			<Tooltip text={TOKEN_BAR_HELP.stop.text} slug={TOKEN_BAR_HELP.stop.slug}>
				<span class="text-micro uppercase tracking-kpi text-[var(--text-3)]">Stop</span>
			</Tooltip>
			<span class="font-mono text-xs text-[var(--text-2)]">{stopDisplay}</span>
		</span>
	</div>
</div>
