<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import type { TokenUsage } from '$lib/types';
	import { formatCount } from '$lib/time';
	import {
		estimateCost,
		formatCostAmount,
		formatRatePerMillion,
		priceNote,
		type CostEstimate
	} from '$lib/modelPricing';
	import Tooltip from '../shared/Tooltip.svelte';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		usage: TokenUsage | null;
		model?: string | null;
	}

	let {
		ref = $bindable(null),
		class: className,
		usage,
		model = null,
		...restProps
	}: Props = $props();

	interface Row {
		key: string;
		label: string;
		tokens: number;
		rateText: string;
		cost: number;
	}

	const estimate = $derived<CostEstimate | null>(
		usage === null ? null : estimateCost(usage, model)
	);
	const rateTip = $derived(
		estimate === null
			? 'USD per million tokens, from the published price of the request model.'
			: priceNote(estimate.lookup, model)
	);
	const priceKnown = $derived(estimate === null || estimate.lookup.known);

	const rows = $derived.by((): Row[] | null => {
		if (estimate === null) return null;
		return estimate.lines.map((l) => ({
			key: l.key,
			label: l.label,
			tokens: l.tokens,
			rateText: formatRatePerMillion(l.perMillion),
			cost: l.cost
		}));
	});

	const totalTokens = $derived(rows === null ? 0 : rows.reduce((acc, r) => acc + r.tokens, 0));
	const totalCost = $derived(rows === null ? 0 : rows.reduce((acc, r) => acc + r.cost, 0));
</script>

<div bind:this={ref} data-slot="cost-breakdown" class={cn('w-full', className)} {...restProps}>
	<table class="w-full border-collapse text-xs">
		<thead>
			<tr>
				<th
					class="pb-2 text-left text-caption font-medium uppercase tracking-kpi text-[var(--text-3)]"
					>Category</th
				>
				<th
					class="pb-2 text-right text-caption font-medium uppercase tracking-kpi text-[var(--text-3)]"
				>
					<Tooltip text="Token count for this category." slug="usage-tokens">Tokens</Tooltip>
				</th>
				<th
					class="pb-2 text-right text-caption font-medium uppercase tracking-kpi text-[var(--text-3)]"
				>
					<Tooltip text={rateTip} slug="request-cost">
						{priceKnown ? 'Rate' : 'Rate (estimate)'}
					</Tooltip>
				</th>
				<th
					class="pb-2 text-right text-caption font-medium uppercase tracking-kpi text-[var(--text-3)]"
				>
					<Tooltip
						text="Tokens × Rate, computed locally — Anthropic does not bill at this granularity."
						slug="request-cost"
					>
						Cost
					</Tooltip>
				</th>
			</tr>
		</thead>
		<tbody>
			{#if rows === null}
				<tr>
					<td class="py-1.5 text-left text-[var(--text-3)]">Uncached input</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
				</tr>
				<tr>
					<td class="py-1.5 text-left text-[var(--text-3)]">Cache read</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
				</tr>
				<tr>
					<td class="py-1.5 text-left text-[var(--text-3)]">Cache create</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
				</tr>
				<tr>
					<td class="py-1.5 text-left text-[var(--text-3)]">Output</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
					<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
				</tr>
				<tr style:border-top="1px solid var(--border-default)">
					<td class="pt-2 text-left font-semibold text-[var(--text-1)]">Total</td>
					<td class="pt-2 text-right font-mono tabular-nums text-[var(--text-3)]">—</td>
					<td></td>
					<td class="pt-2 text-right font-mono font-semibold tabular-nums text-[var(--text-3)]"
						>—</td
					>
				</tr>
			{:else}
				{#each rows as row (row.key)}
					<tr>
						<td class="py-1.5 text-left text-[var(--text-2)]">{row.label}</td>
						<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-1)]"
							>{formatCount(row.tokens)}</td
						>
						<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-3)]"
							>{row.rateText}</td
						>
						<td class="py-1.5 text-right font-mono tabular-nums text-[var(--text-1)]"
							>{formatCostAmount(row.cost)}</td
						>
					</tr>
				{/each}
				<tr style:border-top="1px solid var(--border-default)">
					<td class="pt-2 text-left font-semibold text-[var(--text-1)]">Total</td>
					<td class="pt-2 text-right font-mono font-semibold tabular-nums text-[var(--text-1)]"
						>{formatCount(totalTokens)}</td
					>
					<td></td>
					<td class="pt-2 text-right font-mono font-semibold tabular-nums text-[var(--text-1)]"
						>{formatCostAmount(totalCost)}</td
					>
				</tr>
			{/if}
		</tbody>
	</table>
</div>
