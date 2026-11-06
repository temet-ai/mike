<script lang="ts">
	import { Gauge, Check, X } from '@lucide/svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import ValidationStatusIndicator from '$lib/components/shell/ValidationStatusIndicator.svelte';
	import type { ValidationState } from '$lib/components/shell/ValidationStatusIndicator.svelte';
	import type { ModelBreakdownEntry, SessionTokenUsage } from '$lib/types';
	import { formatCount, formatDuration, parseIso } from '$lib/time';

	export type StatusKind = 'ok' | 'live' | 'error' | 'muted';

	export interface AvgLatency {
		ms: number;
		partial: boolean;
		sampleCount: number;
	}

	interface Props {
		requestCount: number;
		status: { kind: StatusKind; label: string };
		models: ModelBreakdownEntry[];
		tokenUsage: SessionTokenUsage | null;
		firstSeen: string | null;
		lastSeen: string | null;
		avgLatency: AvgLatency | null;
		promptKindCounts: Record<string, number>;
		outcomeCounts: Record<string, number>;
		validation: ValidationState;
		validationSince: string | null;
		now: number;
	}

	let {
		requestCount,
		status,
		models,
		tokenUsage,
		firstSeen,
		lastSeen,
		avgLatency,
		promptKindCounts,
		outcomeCounts,
		validation,
		validationSince,
		now
	}: Props = $props();

	const OUTCOME_ORDER = ['accepted', 'shipped', 'corrected', 'abandoned', 'unknown'] as const;

	const KIND_LABELS: Record<string, string> = {
		'session-recap': 'session recap',
		'teammate-message': 'teammate message',
		'hook-feedback': 'hook feedback',
		'activity-label': 'activity label',
		'background-event': 'background event',
		compaction: 'compaction',
		'analysis-probe': 'analysis probe'
	};

	const operatorAsks = $derived(promptKindCounts.operator ?? 0);

	function plural(n: number, word: string): string {
		return n === 1 ? word : `${word}s`;
	}

	const otherKinds = $derived(
		Object.keys(KIND_LABELS)
			.map((kind) => ({ kind, count: promptKindCounts[kind] ?? 0 }))
			.filter((k) => k.count > 0)
	);

	const totalTokens = $derived(
		tokenUsage === null ? null : tokenUsage.input_tokens + tokenUsage.output_tokens
	);

	const cacheShare = $derived.by((): number | null => {
		if (tokenUsage === null) return null;
		const denom =
			tokenUsage.input_tokens + tokenUsage.cache_read_tokens + tokenUsage.cache_write_tokens;
		if (denom <= 0) return null;
		return Math.round(
			((tokenUsage.cache_read_tokens + tokenUsage.cache_write_tokens) / denom) * 100
		);
	});

	const unanswered = $derived(
		tokenUsage === null ? null : Math.max(0, requestCount - tokenUsage.response_count)
	);

	const tokensByModel = $derived.by((): Map<string, number> => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local: rebuilt each recompute, never read mid-mutation
		const m = new Map<string, number>();
		for (const row of tokenUsage?.by_model ?? []) {
			m.set(row.model, row.input_tokens + row.output_tokens);
		}
		return m;
	});

	function fmtClock(iso: string | null): string {
		const d = parseIso(iso);
		if (!d) return '—';
		const hh = String(d.getHours()).padStart(2, '0');
		const mm = String(d.getMinutes()).padStart(2, '0');
		return `${hh}:${mm}`;
	}

	const spanMs = $derived.by((): number | null => {
		const start = parseIso(firstSeen);
		const end = parseIso(lastSeen);
		if (!start || !end) return null;
		return Math.max(0, end.getTime() - start.getTime());
	});

	function splitUnit(text: string): { value: string; unit: string } {
		const m = /^([\d.,]+)\s*([a-z]*)$/i.exec(text);
		if (!m) return { value: text, unit: '' };
		return { value: m[1] ?? text, unit: m[2] ?? '' };
	}

	const latency = $derived(avgLatency === null ? null : splitUnit(formatDuration(avgLatency.ms)));

	const outcomeEntries = $derived(
		OUTCOME_ORDER.map((k) => ({ kind: k, count: outcomeCounts[k] ?? 0 })).filter((o) => o.count > 0)
	);

	const totalOutcomes = $derived(outcomeEntries.reduce((n, o) => n + o.count, 0));

	const totalModelCalls = $derived(models.reduce((n, m) => n + m.count, 0));

	function modelShare(count: number): number {
		if (totalModelCalls <= 0) return 0;
		return Math.round((count / totalModelCalls) * 100);
	}
</script>

<div class="sm-card edge edge-cyan">
	<div class="sm-card-heading panel-hd">
		<Gauge size={13} aria-hidden="true" />
		Session metrics
	</div>

	<dl class="sm-rows">
		<div class="sm-row">
			<dt class="sm-k"><Tooltip slug="request">Requests</Tooltip></dt>
			<dd class="sm-val">
				<span class="sm-v sm-v--cyan">{formatCount(requestCount)}</span>
				{#if unanswered !== null && unanswered > 0}
					<span class="sm-d">{formatCount(unanswered)} without usage</span>
				{/if}
			</dd>
		</div>

		<div class="sm-row">
			<dt class="sm-k"><Tooltip slug="usage-tokens">Tokens</Tooltip></dt>
			<dd class="sm-val">
				{#if totalTokens === null || totalTokens === 0}
					<span class="sm-none">no usage reported</span>
				{:else}
					<span class="sm-v sm-v--magenta">{formatCount(totalTokens)}</span>
					<span class="sm-d"
						>{formatCount(tokenUsage?.input_tokens ?? 0)} in · {formatCount(
							tokenUsage?.output_tokens ?? 0
						)} out</span
					>
				{/if}
			</dd>
		</div>

		<div class="sm-row">
			<dt class="sm-k"><Tooltip slug="prompt-cache">Cache reuse</Tooltip></dt>
			<dd class="sm-val">
				{#if cacheShare === null}
					<span class="sm-none">no usage reported</span>
				{:else}
					<span class="sm-v">{cacheShare}<span class="sm-u">%</span></span>
					<span class="sm-d"
						>{formatCount(tokenUsage?.cache_read_tokens ?? 0)} read · {formatCount(
							tokenUsage?.cache_write_tokens ?? 0
						)} written</span
					>
				{/if}
			</dd>
		</div>

		<div class="sm-row">
			<dt class="sm-k">
				<Tooltip
					text="First to last captured request in this session, and the elapsed time between them."
					>Span</Tooltip
				>
			</dt>
			<dd class="sm-val">
				<span class="sm-v sm-v--clock"
					>{fmtClock(firstSeen)} <span class="sm-arr">→</span> {fmtClock(lastSeen)}</span
				>
				<span class="sm-d"
					>{spanMs !== null && spanMs > 0 ? formatDuration(spanMs) : 'single instant'}</span
				>
			</dd>
		</div>

		<div class="sm-row">
			<dt class="sm-k">
				<Tooltip
					text="Mean time from request sent to response complete, over the requests loaded here."
					>Avg latency</Tooltip
				>
			</dt>
			<dd class="sm-val">
				{#if latency === null}
					<span class="sm-none">no completed requests</span>
				{:else}
					<span class="sm-v">{latency.value}<span class="sm-u">{latency.unit}</span></span>
					<span class="sm-d"
						>{#if avgLatency?.partial}last{:else}over{/if}
						{formatCount(avgLatency?.sampleCount ?? 0)} requests</span
					>
				{/if}
			</dd>
		</div>

		<div class="sm-row">
			<dt class="sm-k"><Tooltip slug="operator-turn">Operator asks</Tooltip></dt>
			<dd class="sm-val">
				<span class="sm-v">{formatCount(operatorAsks)}</span>
				{#if otherKinds.length > 0}
					<span class="sm-d"
						>{#each otherKinds as k, i (k.kind)}{i > 0 ? ' · ' : ''}{k.count}
							{plural(k.count, KIND_LABELS[k.kind] ?? k.kind)}{/each}</span
					>
				{/if}
			</dd>
		</div>

		<div class="sm-row">
			<dt class="sm-k"><Tooltip slug="request-status">Last request</Tooltip></dt>
			<dd class="sm-val sm-val--chips">
				<span class="sm-chip-backing">
					<span class="sm-chip sm-chip--{status.kind}" data-augmented-ui="tl-clip br-clip border">
						{#if status.kind === 'ok'}
							<Check size={10} aria-hidden="true" />
						{:else if status.kind === 'error'}
							<X size={10} aria-hidden="true" />
						{/if}
						{status.label}
					</span>
				</span>
			</dd>
		</div>

		<div class="sm-row">
			<dt class="sm-k"><Tooltip slug="verifier-steerer">Completion check</Tooltip></dt>
			<dd class="sm-val sm-val--chips">
				{#if validation === null}
					<span class="sm-none">not run</span>
				{:else}
					<ValidationStatusIndicator state={validation} since={validationSince} {now} />
				{/if}
			</dd>
		</div>

		<div class="sm-row sm-row--stack">
			<dt class="sm-k">
				<Tooltip slug="turn-outcome">Outcomes</Tooltip>
				{#if totalOutcomes > 0}
					<span class="sm-k-note">{formatCount(totalOutcomes)} graded</span>
				{/if}
			</dt>
			<dd class="sm-val sm-val--chips sm-val--wrap">
				{#if outcomeEntries.length === 0}
					<span class="sm-none">none graded</span>
				{:else}
					{#each outcomeEntries as o (o.kind)}
						<span class="sm-outcome-backing">
							<span
								class="sm-outcome sm-outcome--{o.kind}"
								data-augmented-ui="tl-clip br-clip border"
							>
								<span class="sm-outcome__k">{o.kind}</span>
								<span class="sm-outcome__v">{o.count}</span>
							</span>
						</span>
					{/each}
				{/if}
			</dd>
		</div>
	</dl>

	<div class="sm-models">
		<div class="sm-k sm-models-k">
			<Tooltip slug="model-id">Models</Tooltip>
			<span class="sm-k-note">{models.length}</span>
		</div>
		{#if models.length === 0}
			<div class="sm-none">no model recorded</div>
		{:else}
			{#each models as m (m.model)}
				{@const tokens = tokensByModel.get(m.model) ?? null}
				<div class="sm-model-row">
					<span class="sm-model-name">{m.model}</span>
					<span class="sm-model-figs">
						{formatCount(m.count)}<span class="sm-model-dim"> · {modelShare(m.count)}%</span>
						{#if tokens !== null}<span class="sm-model-dim"> · {formatCount(tokens)} tok</span>{/if}
					</span>
				</div>
			{/each}
		{/if}
	</div>
</div>

<style>
	.sm-card {
		--edge: 10px;
		min-width: 0;
	}

	.sm-card-heading {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 6px;
	}

	.sm-rows {
		margin: 0;
	}

	.sm-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		padding: 4px 0;
		border-top: 1px solid var(--border-subtle);
	}
	.sm-row:first-of-type {
		border-top: none;
	}
	.sm-row--stack {
		flex-direction: column;
		align-items: stretch;
		gap: 4px;
	}

	.sm-k {
		display: flex;
		align-items: center;
		gap: 4px;
		flex: 0 0 auto;
		font-family: var(--font-mono);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: var(--track-machine, 0.08em);
		color: var(--text-3);
	}
	.sm-k :global(.tooltip-trigger),
	.sm-k :global(.tooltip-link) {
		color: inherit;
		text-decoration: underline dotted;
		text-decoration-color: var(--border-strong);
		text-underline-offset: 3px;
		cursor: help;
	}
	.sm-k :global(.tooltip-link) {
		cursor: pointer;
	}
	.sm-k :global(.tooltip-trigger:hover),
	.sm-k :global(.tooltip-trigger:focus-visible),
	.sm-k :global(.tooltip-link:hover),
	.sm-k :global(.tooltip-link:focus-visible) {
		color: var(--text-strong);
		text-decoration-color: currentColor;
	}

	.sm-k-note {
		margin-left: auto;
		text-transform: none;
		letter-spacing: 0;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}

	.sm-val {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 1px;
		margin: 0;
		min-width: 0;
		text-align: right;
	}
	.sm-val--chips {
		align-items: flex-end;
	}
	.sm-val--wrap {
		flex-direction: row;
		flex-wrap: wrap;
		align-items: center;
		justify-content: flex-start;
		gap: 4px;
	}

	.sm-v {
		font-family: var(--font-display);
		font-size: 15px;
		font-weight: 700;
		line-height: 1.2;
		color: var(--text-strong);
		font-variant-numeric: tabular-nums;
	}
	.sm-v--cyan {
		color: var(--funk-cyan);
	}
	.sm-v--magenta {
		color: var(--funk-magenta);
	}
	.sm-v--clock {
		font-family: var(--font-mono);
		font-size: 12px;
		font-weight: 600;
	}
	.sm-arr {
		color: var(--text-faint);
		font-weight: 400;
	}
	.sm-u {
		font-family: var(--font-mono);
		font-size: 10px;
		font-weight: 400;
		color: var(--text-3);
		margin-left: 1px;
	}

	.sm-d {
		font-family: var(--font-mono);
		font-size: 10px;
		line-height: 1.4;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
		overflow-wrap: anywhere;
	}

	.sm-none {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-faint);
	}

	.sm-chip-backing {
		display: inline-flex;
		background: var(--surface-card);
	}
	.sm-chip {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 7px;
		font-family: var(--font-mono);
		font-size: 10px;
		font-weight: 600;
	}
	.sm-chip--ok {
		background:
			linear-gradient(0deg, var(--status-success-bg), var(--status-success-bg)), var(--surface-card);
		color: var(--status-success-fg);
		--aug-border-bg: var(--status-success-border);
	}
	.sm-chip--live {
		background:
			linear-gradient(0deg, var(--status-warning-bg), var(--status-warning-bg)), var(--surface-card);
		color: var(--status-warning-fg);
		--aug-border-bg: var(--status-warning-border);
		box-shadow:
			0 0 2px var(--status-warning-border),
			0 0 12px var(--status-warning-border);
	}
	@keyframes sm-live-glow {
		0% {
			box-shadow:
				0 0 0 transparent,
				0 0 0 transparent;
		}
		70% {
			box-shadow:
				0 0 2px var(--status-warning-fg),
				0 0 12px var(--status-warning-fg);
		}
		100% {
			box-shadow:
				0 0 2px var(--status-warning-border),
				0 0 12px var(--status-warning-border);
		}
	}
	.sm-chip--live {
		animation: sm-live-glow var(--dur-3) var(--ease-io) both;
	}
	.sm-chip--error {
		background:
			linear-gradient(0deg, var(--status-danger-bg), var(--status-danger-bg)), var(--surface-card);
		color: var(--status-danger-fg);
		--aug-border-bg: var(--status-danger-border);
	}
	.sm-chip--muted {
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card);
		color: var(--text-3);
		--aug-border-bg: var(--border-default);
	}

	.sm-outcome-backing {
		display: inline-flex;
		background: var(--surface-card);
	}
	.sm-outcome {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 1px 5px;
		font-family: var(--font-mono);
		font-size: 10px;
		--aug-border-bg: var(--border-default);
	}
	.sm-outcome--accepted {
		background:
			linear-gradient(0deg, var(--status-success-bg), var(--status-success-bg)), var(--surface-card);
		--aug-border-bg: var(--status-success-border);
		color: var(--status-success-fg);
	}
	.sm-outcome--shipped {
		background:
			linear-gradient(0deg, var(--status-info-bg), var(--status-info-bg)), var(--surface-card);
		--aug-border-bg: var(--status-info-border);
		color: var(--status-info-fg);
	}
	.sm-outcome--corrected {
		background:
			linear-gradient(0deg, var(--status-warning-bg), var(--status-warning-bg)), var(--surface-card);
		--aug-border-bg: var(--status-warning-border);
		color: var(--status-warning-fg);
	}
	.sm-outcome--abandoned {
		background:
			linear-gradient(0deg, var(--status-danger-bg), var(--status-danger-bg)), var(--surface-card);
		--aug-border-bg: var(--status-danger-border);
		color: var(--status-danger-fg);
	}
	.sm-outcome--unknown {
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card);
		--aug-border-bg: var(--border-default);
		color: var(--text-3);
	}
	.sm-outcome__k {
		text-transform: uppercase;
		letter-spacing: 0.04em;
		opacity: 0.75;
	}
	.sm-outcome__v {
		font-variant-numeric: tabular-nums;
		font-weight: 600;
	}

	.sm-models {
		margin-top: 8px;
		border-top: 1px solid var(--border-subtle);
		padding-top: 6px;
	}
	.sm-models-k {
		margin-bottom: 3px;
	}
	.sm-model-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 10px;
		padding: 2px 0;
		font-family: var(--font-mono);
		font-size: 11px;
		font-variant-numeric: tabular-nums;
	}
	.sm-model-name {
		color: var(--text-1);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.sm-model-figs {
		flex: 0 0 auto;
		color: var(--text-strong);
		white-space: nowrap;
	}
	.sm-model-dim {
		color: var(--text-faint);
	}
</style>
