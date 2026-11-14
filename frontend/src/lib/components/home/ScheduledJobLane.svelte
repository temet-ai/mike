<script lang="ts">
	import type { ScheduledJob } from '$lib/api/scheduledJobs';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import {
		GRID_PERCENTS,
		describeLane,
		hoverSegments,
		nowPercent,
		runStrip,
		windowShape,
		type Segment
	} from './jobClock';
	import type { LaneTone } from './scheduledJobs';

	interface Props {
		job: ScheduledJob;
		tone: LaneTone;
		now: number;
		timeZone: string;
		/** Stretches of the axis to light up: what runs with the hovered job. */
		highlight?: Segment[];
		/** Reports the stretches of the axis this job runs in while the pointer is over the lane. */
		onHover?: (segments: Segment[]) => void;
	}

	const { job, tone, now, timeZone, highlight = [], onHover }: Props = $props();

	const nowLeft = $derived(nowPercent(now));

	const shape = $derived(windowShape(job.window));
	const strip = $derived(runStrip(job, now));
	const text = $derived(describeLane(job, now, timeZone));
</script>

<Tooltip {text}>
	{#snippet child({ props })}
		<div
			{...props}
			class="sj-lane"
			data-slot="scheduled-job-lane"
			data-tone={tone}
			role="img"
			aria-label={text}
			onpointerenter={() => onHover?.(hoverSegments(job.window))}
			onpointerleave={() => onHover?.([])}
		>
			<div class="sj-grid" data-slot="scheduled-jobs-grid" aria-hidden="true">
				{#each GRID_PERCENTS as p, i (i)}
					<span
						class="sj-gridline"
						data-midnight={i === 4 ? '' : undefined}
						style="left: calc({p}% - {p / 100}px)"
					></span>
				{/each}
				{#each highlight as seg, i (i)}
					<span
						class="sj-highlight"
						data-slot="scheduled-jobs-highlight"
						style="left: {seg.left}%; width: {seg.width}%"
					></span>
				{/each}
				<span
					class="sj-now"
					data-slot="scheduled-jobs-now"
					style="left: calc({nowLeft}% - {nowLeft / 100}px)"
				></span>
			</div>
			{#if shape.kind === 'none'}
				<span class="sj-lane-none" data-slot="scheduled-job-lane-none">—</span>
			{:else}
				<div class="sj-track">
					{#if shape.kind === 'bar'}
						{#each shape.segments as seg, i (i)}
							<span
								class="sj-bar"
								data-slot="scheduled-job-bar"
								style="left: {seg.left}%; width: {seg.width}%"
							>
								{#if i === 0 && shape.weekday}<span class="sj-weekday">{shape.weekday}</span>{/if}
							</span>
						{/each}
					{:else}
						{#each shape.ticks as tick, i (i)}
							<span class="sj-tick" data-slot="scheduled-job-tick" style="left: {tick}%"></span>
						{/each}
					{/if}
					{#if strip}
						{#each strip.segments as seg, i (i)}
							<span
								class="sj-strip"
								data-slot="scheduled-job-strip"
								data-kind={strip.kind}
								style="left: {seg.left}%; width: {seg.width}%"
							></span>
						{/each}
					{/if}
				</div>
			{/if}
		</div>
	{/snippet}
</Tooltip>

<style>
	.sj-lane {
		--sj-fill: var(--surface-3);
		--sj-edge: var(--border-strong);
		position: relative;
		block-size: var(--space-6);
		min-inline-size: 0;
	}
	.sj-lane[data-tone='live'] {
		--sj-edge: var(--signal-live);
	}
	.sj-lane[data-tone='danger'] {
		--sj-fill: var(--status-danger-bg);
		--sj-edge: var(--status-danger-border);
	}
	.sj-lane[data-tone='warning'] {
		--sj-fill: var(--status-warning-bg);
		--sj-edge: var(--status-warning-border);
	}
	.sj-lane[data-tone='hollow'] {
		--sj-fill: transparent;
		--sj-edge: var(--border-subtle);
	}
	.sj-lane-none {
		position: absolute;
		inset-block-start: 50%;
		inset-inline-start: 0;
		transform: translateY(-50%);
		color: var(--text-3);
		font-size: var(--text-sm);
	}
	.sj-grid {
		position: absolute;
		inset: 0;
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
	.sj-track {
		position: absolute;
		inset-inline: 0;
		block-size: var(--space-3);
		inset-block-start: calc((100% - (var(--space-3) + 2 * var(--space-1))) / 2);
	}
	.sj-bar {
		position: absolute;
		inset-block: 0;
		box-sizing: border-box;
		background: var(--sj-fill);
		border: var(--hairline) solid var(--sj-edge);
		border-radius: 0;
	}
	.sj-weekday {
		display: block;
		padding-inline-start: var(--space-1);
		font-family: var(--font-mono);
		font-size: var(--text-micro);
		line-height: calc(var(--space-3) - 2 * var(--hairline));
		color: var(--text-2);
		white-space: nowrap;
	}
	.sj-tick {
		position: absolute;
		inset-block: 0;
		inline-size: 2px;
		background: var(--sj-edge);
	}
	.sj-strip {
		position: absolute;
		inset-block-start: calc(100% + var(--space-1));
		block-size: var(--space-1);
		min-inline-size: 2px;
		background: var(--signal-complete);
	}
	.sj-strip[data-kind='live'] {
		background: var(--signal-live);
	}
	.sj-strip[data-kind='error'] {
		background: var(--signal-error);
	}
</style>
