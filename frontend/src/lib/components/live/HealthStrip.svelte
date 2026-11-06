<script lang="ts">
	import type { HealthSnapshot } from '$lib/state/liveSystem.svelte';

	interface Props {
		health: HealthSnapshot;
	}

	const { health }: Props = $props();

	const wsClass = $derived(
		health.ws_state === 'connected'
			? 'lp-health-val--ok'
			: health.ws_state === 'stale' || health.ws_state === 'connecting'
				? 'lp-health-val--warn'
				: 'lp-health-val--err'
	);
	const poolSaturated = $derived(health.db_pool_idle !== null && health.db_pool_idle === 0);
</script>

<div class="lp-health-cells" role="list" aria-label="Health indicators">
	<div class="lp-health-cell" role="listitem">
		<span class="lp-health-label">WS</span>
		<span class="lp-health-val {wsClass}">{health.ws_state}</span>
	</div>
	<div class="lp-health-cell" role="listitem">
		<span class="lp-health-label">In-flight</span>
		<span class="lp-health-val" class:lp-health-val--hi={health.in_flight > 0}
			>{health.in_flight}</span
		>
	</div>
	<div class="lp-health-cell" role="listitem">
		<span class="lp-health-label">Receivers</span>
		<span class="lp-health-val"
			>{health.broadcast_receivers}
			{#if health.broadcast_backlog > 0}
				<span class="lp-health-warn">({health.broadcast_backlog} backlog)</span>
			{/if}
		</span>
	</div>
	<div class="lp-health-cell" role="listitem">
		<span class="lp-health-label">DB pool</span>
		{#if health.db_pool_size === null || health.db_pool_idle === null}
			<span class="lp-health-val">n/a</span>
		{:else}
			<span class="lp-health-val" class:lp-health-val--warn={poolSaturated}
				>{health.db_pool_idle} / {health.db_pool_size} idle</span
			>
		{/if}
	</div>
</div>

<style>
	.lp-health-cells {
		display: flex;
		gap: 0;
		flex-wrap: wrap;
	}

	.lp-health-cell {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 0 20px 0 0;
		min-width: 100px;
	}

	.lp-health-cell:first-child {
		padding-left: 0;
	}

	.lp-health-label {
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-3);
	}

	.lp-health-val {
		font-size: 13px;
		color: var(--text-1);
		font-variant-numeric: tabular-nums;
	}

	.lp-health-val--ok {
		color: var(--status-success-fg);
	}
	.lp-health-val--hi {
		color: var(--status-warning-fg);
	}
	.lp-health-val--warn {
		color: var(--status-warning-fg);
	}
	.lp-health-val--err {
		color: var(--status-danger-fg);
	}

	.lp-health-warn {
		font-size: 11px;
		color: var(--status-warning-fg);
	}
</style>
