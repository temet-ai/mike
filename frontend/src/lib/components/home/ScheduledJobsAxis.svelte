<script lang="ts">
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { AXIS_TICKS_WIDE, axisLabels, offsetLabel, offsetMinutes } from './jobClock';

	interface Props {
		now: number;
		timeZone: string;
	}

	const { now, timeZone }: Props = $props();

	const labels = $derived(axisLabels(now, timeZone, AXIS_TICKS_WIDE));
	const offset = $derived(offsetMinutes(now, timeZone));
	const showLocal = $derived(offset !== 0);
	const zoneTip = $derived(`Your browser's time zone: ${timeZone}, ${offsetLabel(offset)}.`);

	function tickClass(i: number): string {
		return i === 0
			? 'sj-tick sj-tick--first'
			: i === labels.length - 1
				? 'sj-tick sj-tick--last'
				: 'sj-tick';
	}
</script>

<div class="sj-axis" data-slot="scheduled-jobs-axis">
	<div class="sj-legend" data-slot="scheduled-jobs-legend">
		<span>UTC</span>
		{#if showLocal}
			<Tooltip text={zoneTip}>
				{#snippet child({ props })}
					<span {...props} class="sj-legend-local">local</span>
				{/snippet}
			</Tooltip>
		{/if}
	</div>
	<div class="sj-labels">
		<div class="sj-label-row sj-label-row--utc">
			{#each labels as label, i (i)}
				<span
					class={tickClass(i)}
					data-odd={i % 2 === 1 ? '' : undefined}
					data-slot="axis-utc"
					style="left: {label.percent}%">{label.utc}</span
				>
			{/each}
		</div>
		{#if showLocal}
			<div class="sj-label-row sj-label-row--local">
				{#each labels as label, i (i)}
					<span
						class={tickClass(i)}
						data-odd={i % 2 === 1 ? '' : undefined}
						data-slot="axis-local"
						style="left: {label.percent}%">{label.local}</span
					>
				{/each}
			</div>
		{/if}
	</div>
</div>

<style>
	.sj-axis {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding-block: var(--space-1);
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		font-variant-numeric: tabular-nums;
		line-height: 14px;
	}
	.sj-legend {
		display: flex;
		gap: var(--space-2);
		color: var(--text-2);
	}
	.sj-legend-local {
		color: var(--text-3);
	}
	.sj-labels {
		display: flex;
		flex-direction: column;
	}
	.sj-label-row {
		position: relative;
		height: 14px;
	}
	.sj-label-row--utc {
		color: var(--text-2);
	}
	.sj-label-row--local {
		color: var(--text-3);
	}
	.sj-tick {
		position: absolute;
		top: 0;
		transform: translateX(-50%);
	}
	.sj-tick--first {
		transform: none;
	}
	.sj-tick--last {
		transform: translateX(-100%);
	}
	@container (max-width: 40rem) {
		.sj-tick[data-odd] {
			display: none;
		}
	}

	@container (min-width: 64rem) {
		.sj-axis {
			grid-column: 1 / -1;
			display: grid;
			grid-template-columns: subgrid;
			align-items: start;
			gap: 0 var(--space-3);
		}
		.sj-legend {
			grid-column: 1 / 3;
			justify-self: end;
			flex-direction: column;
			gap: 0;
			text-align: right;
		}
		.sj-labels {
			grid-column: 3;
		}
	}
</style>
