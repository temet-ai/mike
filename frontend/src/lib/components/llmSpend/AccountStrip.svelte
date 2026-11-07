<script lang="ts">
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { formatUsd } from '$lib/format';
	import type { LlmSpendTodayDto } from '$lib/api/llmSpend';
	import type { OpenRouterUsageDto } from '$lib/api/usage';

	interface Props {
		usage: OpenRouterUsageDto | null;
		today: LlmSpendTodayDto | null;
		todayError: string | null;
		onRetryToday?: () => void;
	}

	const { usage, today, todayError, onRetryToday }: Props = $props();

	const NOT_AVAILABLE = 'Not available from this proxy build yet.';

	const hasRunway = $derived(
		usage !== null && usage.avg_daily_spend_7d_usd !== undefined && usage.days_left !== undefined
	);
	const runwayLabel = $derived.by((): string | null => {
		if (usage === null || !hasRunway) return null;
		const avg = usage.avg_daily_spend_7d_usd as number;
		const days = usage.days_left as number | null;
		if (days === null || avg <= 0) return 'no recent spend to estimate days remaining';
		const rounded = days < 10 ? days.toFixed(1) : Math.round(days).toString();
		return `≈ ${rounded} day${days === 1 ? '' : 's'} at the last 7 days' average of ${formatUsd(avg)}/day`;
	});

	const hasRecordedSplit = $derived(
		usage !== null &&
			usage.recorded_month_usd !== undefined &&
			usage.unrecorded_month_usd !== undefined &&
			usage.usage_monthly !== null &&
			usage.usage_monthly !== undefined
	);
	const unrecordedHighlighted = $derived(
		hasRecordedSplit && (usage?.unrecorded_month_usd ?? 0) > 0.5
	);

	const UNRECORDED_TOOLTIP =
		'Spend OpenRouter billed that no mike call record explains — another instance or tool using the same key, or days the ledger missed.';
	const BILLED_TOOLTIP =
		"What OpenRouter's own usage meter reports for this calendar month, independent of mike's ledger.";
	const RECORDED_TOOLTIP =
		"What mike's own ledger of LLM calls sums to for this calendar month — every steerer, judge, and eval call the proxy made and recorded a cost for.";
	const CEILING_TOOLTIP =
		"The most background LLM workers may spend today. Once today's recorded spend passes this, the proxy refuses further calls from these workers until the day rolls over. No ceiling means unlimited daily spend.";
	const TODAY_TOOLTIP =
		'How much has been spent so far today, and how that compares to the daily ceiling in Settings › Workers.';
</script>

{#if usage !== null && usage.configured}
	<section class="as-strip" aria-labelledby="as-title" data-augmented-ui="tl-clip br-clip border">
		<h3 class="as-title" id="as-title">Spend detail</h3>

		<div class="as-row">
			<span class="as-row-label">Runway</span>
			{#if hasRunway}
				<span class="as-row-value">{runwayLabel}</span>
			{:else}
				<span class="as-row-value as-row-value--muted">{NOT_AVAILABLE}</span>
			{/if}
		</div>

		<div class="as-row">
			<span class="as-row-label"
				>Today
				<Tooltip text={TODAY_TOOLTIP}><span class="as-hint" aria-hidden="true">?</span></Tooltip
				></span
			>
			{#if todayError !== null}
				<span class="as-row-value as-row-value--error"
					>Could not load: {todayError}
					{#if onRetryToday}
						<button type="button" class="as-inline-retry" onclick={() => onRetryToday?.()}
							>Retry</button
						>
					{/if}
				</span>
			{:else if today === null}
				<span class="as-row-value" aria-busy="true">Loading...</span>
			{:else}
				<span class="as-row-value">
					{formatUsd(today.spent_usd)}
					<span class="as-row-of"
						>of
						<Tooltip text={CEILING_TOOLTIP}><span class="as-term">ceiling</span></Tooltip>
						{today.ceiling_enabled ? formatUsd(today.ceiling_usd) : 'none set'}</span
					>
					{#if today.over_ceiling}
						<span class="as-badge as-badge--danger">over ceiling</span>
					{:else if today.over_alert_threshold}
						<span class="as-badge as-badge--warning">near ceiling</span>
					{/if}
				</span>
			{/if}
		</div>

		<div class="as-row as-row--wrap">
			<span class="as-row-label">This month</span>
			{#if hasRecordedSplit}
				<span class="as-row-value">
					<Tooltip text={BILLED_TOOLTIP}><span class="as-term">Billed</span></Tooltip> by OpenRouter
					<strong>{formatUsd(usage?.usage_monthly ?? null)}</strong>
					<span class="as-sep">·</span>
					<Tooltip text={RECORDED_TOOLTIP}><span class="as-term">Recorded</span></Tooltip> by mike
					<strong>{formatUsd(usage?.recorded_month_usd ?? null)}</strong>
					<span class="as-sep">·</span>
					<Tooltip text={UNRECORDED_TOOLTIP}
						><span class="as-term" class:as-term--danger={unrecordedHighlighted}>Unrecorded</span
						></Tooltip
					>
					<strong class:as-unrecorded--danger={unrecordedHighlighted}
						>{formatUsd(usage?.unrecorded_month_usd ?? null)}</strong
					>
				</span>
			{:else}
				<span class="as-row-value as-row-value--muted">{NOT_AVAILABLE}</span>
			{/if}
		</div>
	</section>
{/if}

<style>
	.as-strip {
		--edge-tl: 8px;
		--edge-br: 10px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg) 0%,
			var(--data-teal-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 14px 16px 16px;
		background: var(--surface-card);
	}
	.as-title {
		font-family: var(--font-display);
		font-size: 13px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 0;
	}
	.as-row {
		display: flex;
		align-items: baseline;
		gap: 10px;
		flex-wrap: wrap;
		padding-top: 10px;
		border-top: 1px solid var(--border-subtle);
	}
	.as-row:first-of-type {
		padding-top: 0;
		border-top: none;
	}
	.as-row--wrap .as-row-value {
		flex-basis: 100%;
	}
	.as-row-label {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		flex: 0 0 90px;
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}
	.as-hint {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 13px;
		height: 13px;
		border-radius: 50%;
		border: 1px solid var(--text-3);
		font-size: 9px;
		color: var(--text-3);
		cursor: help;
	}
	.as-row-value {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
		font-size: 13px;
		color: var(--text-1);
		font-variant-numeric: tabular-nums;
	}
	.as-row-value--muted {
		color: var(--text-3);
		font-style: italic;
	}
	.as-row-value--error {
		color: var(--status-danger-fg);
	}
	.as-row-value strong {
		color: var(--text-1);
		font-weight: 600;
	}
	.as-row-of {
		font-size: 12px;
		color: var(--text-3);
		display: inline-flex;
		align-items: center;
		gap: 3px;
	}
	.as-term {
		text-decoration: underline dotted;
		text-underline-offset: 2px;
		cursor: help;
	}
	.as-term--danger {
		color: var(--status-danger-fg);
	}
	.as-inline-retry {
		background: transparent;
		border: 0;
		padding: 0;
		font: inherit;
		text-decoration: underline;
		color: var(--status-danger-fg);
		cursor: pointer;
	}
	.as-sep {
		color: var(--text-3);
	}
	.as-row-value strong.as-unrecorded--danger {
		color: var(--status-danger-fg);
	}
	.as-badge {
		display: inline-flex;
		align-items: center;
		padding: 2px 7px;
		border-radius: 4px;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.as-badge--danger {
		background: var(--status-danger-bg);
		color: var(--status-danger-fg);
		border: 1px solid var(--status-danger-border);
	}
	.as-badge--warning {
		background: var(--status-warning-bg);
		color: var(--status-warning-fg);
		border: 1px solid var(--status-warning-border);
	}
</style>
