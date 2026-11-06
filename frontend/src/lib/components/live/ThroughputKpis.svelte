<script lang="ts">
	import type { ThroughputSnapshot } from '$lib/state/liveSystem.svelte';
	import Sparkline from './Sparkline.svelte';

	interface Props {
		throughput: ThroughputSnapshot;
		stale?: boolean;
	}

	const { throughput, stale = false }: Props = $props();

	const errClass = $derived(
		throughput.error_rate_pct >= 10
			? 'lp-kpi-val--err'
			: throughput.error_rate_pct > 0
				? 'lp-kpi-val--warn'
				: 'lp-kpi-val--ok'
	);
</script>

<div class="lp-kpi-row">
	<div class="lp-kpi">
		<div class="lp-kpi-val" class:lp-td-stale={stale}>
			{throughput.req_per_min.toFixed(1)}
		</div>
		<div class="lp-kpi-label">req / min</div>
		<Sparkline values={throughput.req_history} stroke="var(--status-info-fg)" kpi />
	</div>
	<div class="lp-kpi">
		<div class="lp-kpi-val" class:lp-td-stale={stale}>
			{(throughput.tokens_per_sec / 1000).toFixed(1)}k
		</div>
		<div class="lp-kpi-label">tok / sec</div>
		<Sparkline values={throughput.tokens_history} stroke="var(--data-teal-fg)" kpi />
	</div>
	<div class="lp-kpi">
		<div class="lp-kpi-val {errClass}" class:lp-td-stale={stale}>
			{throughput.error_rate_pct.toFixed(1)}%
		</div>
		<div class="lp-kpi-label">error rate</div>
	</div>
</div>

<style>
	.lp-kpi-row {
		display: flex;
		gap: 24px;
		align-items: flex-start;
	}

	.lp-kpi {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 80px;
	}

	.lp-kpi-val {
		font-size: 22px;
		font-weight: 700;
		letter-spacing: -0.02em;
		color: var(--text-strong);
		line-height: 1;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
	}

	.lp-kpi-val--ok {
		color: var(--status-success-fg);
	}
	.lp-kpi-val--warn {
		color: var(--status-warning-fg);
	}
	.lp-kpi-val--err {
		color: var(--status-danger-fg);
	}

	.lp-kpi-label {
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-3);
	}

	.lp-td-stale {
		opacity: 0.5;
	}
</style>
