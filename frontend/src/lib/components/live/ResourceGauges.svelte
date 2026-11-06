<script lang="ts">
	import { formatBytes } from '$lib/format';
	import type { MemorySnapshot } from '$lib/state/liveSystem.svelte';
	import Sparkline from './Sparkline.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';

	interface Props {
		memory: MemorySnapshot;
		stale?: boolean;
	}

	const { memory, stale = false }: Props = $props();

	const usedPct = $derived(
		memory.max_store_bytes > 0
			? Math.round((memory.entries_total_bytes / memory.max_store_bytes) * 100)
			: 0
	);
	const overBudget = $derived(
		memory.max_store_bytes > 0 && memory.entries_total_bytes > memory.max_store_bytes
	);
	const fillClass = $derived(
		overBudget || usedPct >= 90
			? 'lp-gauge-fill--crit'
			: usedPct >= 70
				? 'lp-gauge-fill--warn'
				: 'lp-gauge-fill--ok'
	);
	const sparkStroke = $derived(
		overBudget || usedPct >= 90
			? 'var(--status-danger-fg)'
			: usedPct >= 70
				? 'var(--status-warning-fg)'
				: 'var(--status-success-fg)'
	);

	const FIELD_COLORS: Record<string, string> = {
		request_body: 'var(--data-viz-1-fg)',
		response_body: 'var(--data-viz-2-fg)',
		filtered_request_body: 'var(--data-viz-3-fg)',
		tool_calls: 'var(--data-viz-4-fg)',
		tool_results: 'var(--data-viz-5-fg)',
		tool_results_in_body: 'var(--data-viz-6-fg)',
		system_prompt_blocks: 'var(--data-viz-7-fg)'
	};
	function fieldColor(field: string): string {
		return FIELD_COLORS[field] ?? 'var(--text-3)';
	}

	function shareFraction(bytes: number): number {
		return memory.entries_total_bytes > 0 ? bytes / memory.entries_total_bytes : 0;
	}
</script>

<div class="lp-gauges">
	<div class="lp-gauge" class:lp-gauge--alert={overBudget}>
		<div class="lp-gauge-header">
			<span class="lp-gauge-label">
				<Tooltip text="Bytes held in the in-memory capture store vs the configured budget">
					Store
				</Tooltip>
			</span>
			<span class="lp-gauge-value" class:lp-gauge-value--alert={overBudget}>
				{formatBytes(memory.entries_total_bytes)}
			</span>
			{#if memory.max_store_bytes > 0}
				<span class="lp-gauge-of">/ {formatBytes(memory.max_store_bytes)}</span>
				<span class="lp-gauge-pct" class:lp-gauge-pct--alert={overBudget}>{usedPct}%</span>
			{:else}
				<span class="lp-gauge-of">/ unbudgeted</span>
			{/if}
		</div>
		{#if memory.max_store_bytes > 0}
			<div
				class="lp-gauge-bar"
				role="meter"
				aria-valuenow={Math.min(usedPct, 100)}
				aria-valuemin={0}
				aria-valuemax={100}
				aria-label="Store usage {usedPct}%{stale ? ' (stale)' : ''}"
			>
				<div
					class="lp-gauge-fill {fillClass}"
					class:lp-gauge-fill--stale={stale}
					style:width="{Math.min(usedPct, 100)}%"
				></div>
			</div>
		{/if}
		{#if memory.history.length > 0}
			<div class="lp-sparkline-row">
				<Sparkline values={memory.history} stroke={sparkStroke} />
				<span class="lp-sparkline-label">{memory.history.length} samples</span>
			</div>
		{/if}
	</div>

	<div class="lp-gauge">
		<div class="lp-gauge-header">
			<span class="lp-gauge-label">
				<Tooltip text="Process resident set size — total OS memory held by this process">
					RSS
				</Tooltip>
			</span>
			<span class="lp-gauge-value">
				{memory.rss_bytes > 0 ? formatBytes(memory.rss_bytes) : 'n/a'}
			</span>
		</div>
	</div>
</div>

<div class="lp-breakdown" class:lp-breakdown--stale={stale}>
	<span class="lp-breakdown-hd">Store by field</span>
	<div class="lp-share-bar" role="img" aria-label="Store composition by field">
		{#each memory.breakdown as row (row.field)}
			{@const frac = shareFraction(row.bytes)}
			{#if frac > 0}
				<span
					class="lp-share-seg"
					style:width="{(frac * 100).toFixed(3)}%"
					style:background={fieldColor(row.field)}
					title="{row.field} · {formatBytes(row.bytes)} · {(frac * 100).toFixed(1)}%"
				></span>
			{/if}
		{/each}
	</div>
	<ul class="lp-legend" aria-label="Store breakdown by field">
		{#each memory.breakdown as row (row.field)}
			{@const frac = shareFraction(row.bytes)}
			<li class="lp-legend-item">
				<span class="lp-legend-swatch" style:background={fieldColor(row.field)} aria-hidden="true"
				></span>
				<span class="lp-legend-field">{row.field}</span>
				<span class="lp-legend-bytes">{formatBytes(row.bytes)}</span>
				<span class="lp-legend-share">{(frac * 100).toFixed(1)}%</span>
			</li>
		{/each}
	</ul>
</div>

<style>
	.lp-gauges {
		display: flex;
		gap: 16px;
		margin-bottom: 12px;
	}

	.lp-gauge {
		flex: 1 1 0;
		min-width: 0;
	}

	.lp-gauge--alert {
		border-left: 2px solid var(--status-warning-fg);
		padding-left: 8px;
	}

	.lp-gauge-header {
		display: flex;
		align-items: baseline;
		gap: 6px;
		margin-bottom: 6px;
	}

	.lp-gauge-label {
		font-size: 10px;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-3);
	}

	.lp-gauge-value {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-strong);
		letter-spacing: -0.01em;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
	}

	.lp-gauge-value--alert {
		color: var(--status-warning-fg);
	}

	.lp-gauge-of {
		font-size: 11px;
		color: var(--text-3);
	}

	.lp-gauge-pct {
		font-size: 11px;
		color: var(--text-2);
		margin-left: auto;
	}

	.lp-gauge-pct--alert {
		color: var(--status-warning-fg);
	}

	.lp-gauge-bar {
		height: 4px;
		background: var(--surface-2);
		border-radius: 2px;
		overflow: hidden;
	}

	.lp-gauge-fill {
		height: 100%;
		border-radius: 2px;
		transition: width 400ms ease-out;
	}

	.lp-gauge-fill--ok {
		background: var(--status-success-fg);
	}
	.lp-gauge-fill--warn {
		background: var(--status-warning-fg);
	}
	.lp-gauge-fill--crit {
		background: var(--status-danger-fg);
	}
	.lp-gauge-fill--stale {
		opacity: 0.4;
	}

	.lp-sparkline-row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 4px;
	}

	.lp-sparkline-label {
		font-size: 10px;
		color: var(--text-3);
	}

	.lp-breakdown {
		max-width: 640px;
	}

	.lp-breakdown--stale {
		opacity: 0.6;
	}

	.lp-breakdown-hd {
		display: block;
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-3);
		margin-bottom: 6px;
	}

	.lp-share-bar {
		display: flex;
		width: 100%;
		height: 8px;
		border-radius: 3px;
		overflow: hidden;
		background: var(--surface-2);
	}

	.lp-share-seg {
		height: 100%;
		min-width: 1px;
	}
	.lp-share-seg + .lp-share-seg {
		border-left: 1px solid var(--surface-card);
	}

	.lp-legend {
		list-style: none;
		margin: 8px 0 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 4px 16px;
	}

	.lp-legend-item {
		display: inline-flex;
		align-items: baseline;
		gap: 6px;
		font-size: 11px;
		min-width: 0;
	}

	.lp-legend-swatch {
		flex-shrink: 0;
		width: 8px;
		height: 8px;
		border-radius: 2px;
		align-self: center;
	}

	.lp-legend-field {
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		word-break: break-word;
	}

	.lp-legend-bytes {
		color: var(--text-2);
		font-variant-numeric: tabular-nums;
	}

	.lp-legend-share {
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
</style>
