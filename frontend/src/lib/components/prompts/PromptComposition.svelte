<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import type { RequestDetail } from '$lib/types';
	import { formatCount } from '$lib/time';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		detail: RequestDetail;
	}

	let { ref = $bindable(null), class: className, detail, ...restProps }: Props = $props();

	const inputTokens = $derived(detail.token_usage?.input_tokens ?? null);
	const maxTokens = $derived(detail.params.max_tokens);

	const budgetPercent = $derived.by((): number | null => {
		if (inputTokens === null || maxTokens === null || maxTokens <= 0) {
			return null;
		}
		return Math.min(999, Math.round((inputTokens / maxTokens) * 100));
	});

	const budgetColor = $derived.by((): string => {
		if (budgetPercent === null) return 'var(--text-3)';
		if (budgetPercent >= 90) return 'var(--status-danger-fg)';
		if (budgetPercent >= 60) return 'var(--status-warning-fg)';
		return 'var(--status-success-fg)';
	});

	const budgetBarWidth = $derived.by((): string => {
		if (budgetPercent === null) return '0%';
		return `${Math.min(100, budgetPercent)}%`;
	});

	const budgetAriaLabel = $derived(
		budgetPercent === null
			? 'Context budget usage: unavailable'
			: `Context budget usage: ${budgetPercent}% of max_tokens`
	);
</script>

<div
	bind:this={ref}
	data-slot="prompt-composition"
	class={cn('flex flex-col gap-4', className)}
	{...restProps}
>
	<section data-slot="context-budget" class="flex flex-col gap-2">
		<header class="flex items-baseline justify-between gap-2">
			<h3 class="text-label font-medium uppercase tracking-kpi text-[var(--text-3)]">
				Context Budget
			</h3>
			<span class="font-mono tabular-nums text-[var(--text-3)]" style:font-size="10px">
				input_tokens vs params.max_tokens
			</span>
		</header>
		{#if budgetPercent === null}
			<p class="text-xs text-[var(--text-3)]">
				Budget ratio unavailable — {inputTokens === null
					? 'no usage recorded'
					: 'max_tokens not set on this request'}.
			</p>
		{:else}
			<div class="flex items-center gap-3">
				<div
					class="relative h-2 flex-1 overflow-hidden rounded"
					style:background-color="var(--surface-2)"
					role="progressbar"
					aria-label={budgetAriaLabel}
					aria-valuenow={budgetPercent}
					aria-valuemin="0"
					aria-valuemax="100"
				>
					<div
						class="absolute inset-y-0 left-0 rounded"
						style:background-color={budgetColor}
						style:width={budgetBarWidth}
					></div>
				</div>
				<span
					class="shrink-0 font-mono font-semibold tabular-nums text-[var(--budget-color)]"
					style:--budget-color={budgetColor}
					style:font-size="12px"
				>
					{budgetPercent}%
				</span>
			</div>
			<p class="font-mono tabular-nums text-[var(--text-3)]" style:font-size="10px">
				{formatCount(inputTokens ?? 0)} input tokens / {formatCount(maxTokens ?? 0)} max_tokens
			</p>
		{/if}
	</section>

	<section data-slot="turn-growth" class="flex flex-col gap-2">
		<header class="flex items-baseline justify-between gap-2">
			<h3 class="text-label font-medium uppercase tracking-kpi text-[var(--text-3)]">
				Turn-over-turn Growth
			</h3>
		</header>
		<p class="text-xs text-[var(--text-3)]">
			Requires session-level aggregation across multiple requests — the detail view carries one
			request at a time. Planned follow-up: surface this on the session timeline where the full turn
			sequence is available.
		</p>
	</section>
</div>
