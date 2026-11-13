<script lang="ts">
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import {
		AXIS_TICKS_WIDE,
		axisLabels,
		axisPercent,
		offsetLabel,
		offsetMinutes,
		type Segment
	} from './jobClock';

	interface Props {
		now: number;
		timeZone: string;
		/** Stretches of the axis to light up: what runs with the hovered job. */
		highlight?: Segment[];
	}

	const { now, timeZone, highlight = [] }: Props = $props();

	const labels = $derived(axisLabels(now, timeZone, AXIS_TICKS_WIDE));
	const offset = $derived(offsetMinutes(now, timeZone));
	const showLocal = $derived(offset !== 0);
	const nowMinute = $derived.by(() => {
		const d = new Date(now);
		return d.getUTCHours() * 60 + d.getUTCMinutes() + d.getUTCSeconds() / 60;
	});
	const nowLeft = $derived(axisPercent(nowMinute));
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

<div class="sj-grid" data-slot="scheduled-jobs-grid" aria-hidden="true">
	{#each labels as label, i (i)}
		<span
			class="sj-gridline"
			data-midnight={i === 4 ? '' : undefined}
			style="left: {label.percent}%"
		></span>
	{/each}
	{#each highlight as seg, i (i)}
		<span
			class="sj-highlight"
			data-slot="scheduled-jobs-highlight"
			style="left: {seg.left}%; width: {seg.width}%"
		></span>
	{/each}
	<span class="sj-now" data-slot="scheduled-jobs-now" style="left: {nowLeft}%"></span>
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

	.sj-grid {
		position: absolute;
		inset-block: 0;
		inset-inline: var(--space-3);
		pointer-events: none;
	}
	.sj-gridline,
	.sj-now {
		position: absolute;
		inset-block: 0;
		inline-size: var(--hairline);
		background: var(--border-subtle);
	}
	.sj-gridline[data-midnight] {
		background: var(--border-default);
	}
	.sj-now {
		background: var(--signal-active);
	}
	.sj-highlight {
		position: absolute;
		inset-block: 0;
		background: var(--interactive-hover-surface);
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
		.sj-grid {
			inset-inline: 0;
			grid-column: 3;
		}
	}
</style>
