<script lang="ts">
	import type { ConsolidationRun, RunKind } from '$lib/api/memoryDecisions';
	import { formatRelative, formatClockAbsolute } from '$lib/time';

	interface Props {
		runs: ConsolidationRun[];
		nightlyHourUtc: number | null;
		now: number;
	}

	let { runs, nightlyHourUtc, now }: Props = $props();

	const KIND_LABELS: Record<RunKind, string> = {
		session: 'Last session run',
		nightly: 'Last nightly run',
		gc: 'Last GC pass'
	};

	const RUN_KINDS = ['session', 'nightly', 'gc'] as const satisfies readonly RunKind[];

	function mostRecent(kind: RunKind): ConsolidationRun | null {
		return runs.find((r) => r.kind === kind) ?? null;
	}

	const stripItems = $derived(
		RUN_KINDS.map((kind) => ({ kind, label: KIND_LABELS[kind], run: mostRecent(kind) }))
	);

	function statusTone(
		status: ConsolidationRun['status']
	): 'active' | 'warning' | 'error' | 'muted' {
		if (status === 'completed') return 'active';
		if (status === 'failed') return 'error';
		if (status === 'partial') return 'warning';
		return 'muted';
	}

	const nextNightly = $derived.by((): Date | null => {
		if (nightlyHourUtc === null) return null;
		const next = new Date(now);
		next.setUTCHours(nightlyHourUtc, 0, 0, 0);
		if (next.getTime() <= now) next.setUTCDate(next.getUTCDate() + 1);
		return next;
	});
</script>

<div
	class="rs-strip"
	data-design="funk"
	data-augmented-ui="tl-clip br-clip border"
	aria-label="Consolidation run status"
>
	{#each stripItems as item (item.kind)}
		<div class="rs-item">
			<span class="rs-item__label">{item.label}</span>
			{#if item.run}
				<span class="badge-nx badge-{statusTone(item.run.status)}">{item.run.status}</span>
				<span
					class="rs-item__when font-mono text-micro tabular-nums"
					title={formatClockAbsolute(item.run.started_at)}
				>
					{formatRelative(item.run.started_at, now)}
				</span>
			{:else}
				<span class="rs-item__none">never run</span>
			{/if}
		</div>
	{/each}
	{#if nextNightly}
		<div class="rs-item">
			<span class="rs-item__label">Next nightly (expected)</span>
			<span
				class="rs-item__when font-mono text-micro tabular-nums"
				title={nextNightly.toISOString()}
			>
				{formatClockAbsolute(nextNightly.toISOString())}
			</span>
		</div>
	{/if}
</div>

<style>
	.rs-strip {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--edge-line);
		background: var(--edge-surface);
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-5);
		padding: var(--edge-pad, calc(var(--edge) + var(--space-1)));
	}

	.rs-item {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 140px;
	}

	.rs-item__label {
		font-size: var(--text-label);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: var(--track-display);
	}

	.rs-item__when {
		color: var(--text-1);
	}

	.rs-item__none {
		font-size: var(--text-xs);
		color: var(--text-faint);
		font-style: italic;
	}
</style>
