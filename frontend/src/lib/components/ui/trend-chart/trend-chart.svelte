<script lang="ts">
	import { Minus, TrendingDown, TrendingUp } from '@lucide/svelte';
	import { cn } from '$lib/utils';
	import { StatusMark } from '$lib/components/ui/status-mark';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import {
		buildPlot,
		formatDay,
		nearestRun,
		summariseChange,
		valueAt,
		type TrendDirection,
		type TrendPoint
	} from './trend';

	interface Props {
		label: string;
		/** Explanation of the measure, shown from the label. */
		tooltip?: string;
		/** Oldest first. */
		points: TrendPoint[];
		format: (n: number) => string;
		/** Receives the signed change and returns its text, minus sign included. */
		formatDelta: (d: number) => string;
		better?: TrendDirection;
		/** Shared x domain, epoch ms. */
		domain: [number, number];
		/** Epoch ms of the run pointed at; bind it across a group of charts. */
		pointAt?: number | null;
		/** Plot height in px. */
		height?: number;
		class?: string;
	}

	let {
		label,
		tooltip,
		points,
		format,
		formatDelta,
		better,
		domain,
		pointAt = $bindable(null),
		height = 56,
		class: className
	}: Props = $props();

	const plot = $derived(buildPlot(points, domain, height));
	const change = $derived(summariseChange(points, better, formatDelta));
	const newest = $derived(points.length > 0 ? points[points.length - 1]! : null);
	const showAll = $derived(plot.points.length <= 60);
	const span = $derived(domain[1] - domain[0] || 1);
	const pointed = $derived(
		pointAt === null || pointAt === undefined ? null : valueAt(plot.points, pointAt)
	);
	const pointX = $derived(
		pointAt === null || pointAt === undefined
			? 0
			: Math.min(100, Math.max(0, ((pointAt - domain[0]) / span) * 100))
	);

	function move(event: PointerEvent): void {
		const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
		if (box.width === 0) return;
		const target = domain[0] + ((event.clientX - box.left) / box.width) * span;
		const run = nearestRun(
			plot.points.map((p) => p.t),
			target
		);
		if (run !== null) pointAt = run;
	}

	const Icon = $derived(
		change.kind === 'down' ? TrendingDown : change.kind === 'up' ? TrendingUp : Minus
	);
</script>

<div class={cn('tc', className)} data-slot="trend-chart">
	<div class="tc__label machine">
		{#if tooltip}
			<Tooltip text={tooltip}><span>{label}</span></Tooltip>
		{:else}
			<span>{label}</span>
		{/if}
	</div>
	<div class="tc__value">{newest ? format(newest.value) : '—'}</div>
	<StatusMark class="tc__change" icon={Icon} tone={change.tone} label={change.label} />
	<div class="tc__chart">
		<div
			class="tc__plot"
			style="height: {height}px"
			aria-hidden="true"
			onpointermove={move}
			onpointerleave={() => (pointAt = null)}
		>
			<svg viewBox="0 0 100 {height}" preserveAspectRatio="none" width="100%" {height}>
				<line x1="0" x2="100" y1="0" y2="0" class="tc__rule" />
				<line x1="0" x2="100" y1={height} y2={height} class="tc__rule" />
				{#if plot.area}<path d={plot.area} class="tc__area" />{/if}
				{#if plot.path}<path d={plot.path} class="tc__line" />{/if}
			</svg>
			{#if showAll}
				{#each plot.points as p (p.t)}
					<span class="tc__dot" style="left: {p.x}%; top: {p.y}px"></span>
				{/each}
			{/if}
			{#if plot.points.length > 0}
				{@const last = plot.points[plot.points.length - 1]!}
				<span class="tc__dot tc__dot--newest" style="left: {last.x}%; top: {last.y}px"></span>
			{/if}
			{#if pointed}
				<span class="tc__pointer" style="left: {pointX}%"></span>
				<span class="tc__dot tc__dot--pointed" style="left: {pointX}%; top: {pointed.y}px"></span>
				<span
					class="tc__pointed-value"
					class:tc__pointed-value--left={pointX > 66}
					style="left: {pointX}%; top: {pointed.y}px"
				>
					{format(pointed.value)}
				</span>
			{/if}
		</div>
		<div class="tc__gutter" aria-hidden="true">
			{#if plot.flat}
				<span class="tc__y tc__y--mid">{newest ? format(plot.max) : ''}</span>
			{:else}
				<span class="tc__y tc__y--top">{format(plot.max)}</span>
				<span class="tc__y tc__y--bottom">{format(plot.min)}</span>
			{/if}
		</div>
	</div>
	<div class="tc__axis" aria-hidden="true">
		<span>{formatDay(domain[0])}</span>
		<span>{formatDay(domain[1])}</span>
	</div>
</div>

<style>
	.tc {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
	}
	.tc__label {
		font-size: var(--text-label);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
	.tc__value {
		font-family: var(--font-display);
		font-size: var(--text-xl);
		font-weight: var(--weight-display);
		line-height: var(--leading-tight);
		font-variant-numeric: tabular-nums;
		color: var(--text-1);
	}
	.tc :global(.tc__change) {
		font-size: var(--text-xs);
	}
	.tc__chart {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 3.5rem;
		margin-top: var(--space-1);
	}
	.tc__plot {
		position: relative;
	}
	.tc__plot svg {
		display: block;
		overflow: visible;
	}
	.tc__rule {
		stroke: var(--border-subtle);
		stroke-width: 1;
		vector-effect: non-scaling-stroke;
	}
	.tc__area {
		fill: var(--surface-2);
		stroke: none;
	}
	.tc__line {
		fill: none;
		stroke: var(--text-2);
		stroke-width: 1.5;
		vector-effect: non-scaling-stroke;
	}
	.tc__dot {
		position: absolute;
		width: 2px;
		height: 2px;
		margin: -1px 0 0 -1px;
		background: var(--text-2);
		pointer-events: none;
	}
	.tc__dot--newest {
		width: 4px;
		height: 4px;
		margin: -2px 0 0 -2px;
		background: var(--text-1);
	}
	.tc__dot--pointed {
		width: 4px;
		height: 4px;
		margin: -2px 0 0 -2px;
		background: var(--signal-info);
	}
	.tc__pointer {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 1px;
		margin-left: -0.5px;
		background: var(--signal-info);
		pointer-events: none;
	}
	.tc__pointed-value {
		position: absolute;
		transform: translate(6px, -50%);
		padding: 0 var(--space-1);
		background: var(--surface-card);
		color: var(--text-1);
		font-size: var(--text-micro);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		pointer-events: none;
	}
	.tc__pointed-value--left {
		transform: translate(calc(-100% - 6px), -50%);
	}
	.tc__gutter {
		position: relative;
		padding-left: var(--space-1);
	}
	.tc__y {
		position: absolute;
		left: var(--space-1);
		font-size: var(--text-micro);
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
		line-height: 1;
		white-space: nowrap;
	}
	.tc__y--top {
		top: 0;
		transform: translateY(-50%);
	}
	.tc__y--bottom {
		bottom: 0;
		transform: translateY(50%);
	}
	.tc__y--mid {
		top: 50%;
		transform: translateY(-50%);
	}
	.tc__axis {
		display: flex;
		justify-content: space-between;
		padding-right: 3.5rem;
		font-size: var(--text-micro);
		color: var(--text-3);
	}
</style>
