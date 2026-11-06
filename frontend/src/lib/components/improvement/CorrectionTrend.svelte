<!--
@component
Weekly corrected share, counted exactly as the Judge page's label alarm counts it: every real turn
the judge labelled in each seven days, in every project (`sql_label_mix.rs`). The chart has a
percent axis, a date under each week, the latest value at the line's end, and a tooltip on hover
or keyboard focus; the same numbers are in the table below it, so nothing is hover-only. A thin
vertical line marks a week when the judges writing the labels changed, and the sentence above the
chart names those weeks. Lesson and directive decisions are grey diamonds on the baseline and are
listed as links under the chart.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { formatDay } from '$lib/components/eval/evalReview';
	import type { TrendMarker, TrendPoint } from '$lib/api/improvement';
	import {
		changeSentence,
		judgeChanged,
		lastDayLabel,
		sharePct,
		shareValue,
		weekIndexOf,
		weekLabel,
		xTickIndices,
		yTicks
	} from './correctionTrend';

	interface Props {
		weekly: TrendPoint[];
		markers: TrendMarker[];
		windowLabel: string;
		error: string | null;
		onRetry: () => void;
	}

	const { weekly, markers, windowLabel, error, onRetry }: Props = $props();

	const H = 200;
	const M = { top: 12, right: 44, bottom: 28, left: 40 };
	const plotH = H - M.top - M.bottom;

	let width = $state(640);
	let active = $state<number | null>(null);

	const plotW = $derived(Math.max(120, width - M.left - M.right));
	const labelled = $derived(weekly.filter((p) => p.n > 0));
	const ticks = $derived(yTicks(Math.max(0, ...labelled.map((p) => shareValue(p) ?? 0))));
	const yTop = $derived(ticks.at(-1) ?? 10);
	const xTicks = $derived(xTickIndices(weekly.length, plotW));

	function x(i: number): number {
		return weekly.length <= 1 ? plotW / 2 : (i / (weekly.length - 1)) * plotW;
	}

	function y(pct: number): number {
		return plotH - (pct / yTop) * plotH;
	}

	const path = $derived.by(() => {
		let d = '';
		let pen = false;
		weekly.forEach((p, i) => {
			const v = shareValue(p);
			if (v === null) {
				pen = false;
				return;
			}
			d += `${pen ? 'L' : 'M'}${x(i).toFixed(1)},${y(v).toFixed(1)} `;
			pen = true;
		});
		return d.trim();
	});

	const lastIndex = $derived(weekly.findLastIndex((p) => p.n > 0));
	const latest = $derived(lastIndex === -1 ? null : weekly[lastIndex]!);
	const previous = $derived.by(() => {
		for (let i = lastIndex - 1; i >= 0; i--) if (weekly[i]!.n > 0) return weekly[i]!;
		return null;
	});
	const directionWord = $derived.by((): string => {
		const a = latest ? shareValue(latest) : null;
		const b = previous ? shareValue(previous) : null;
		if (a === null || b === null || sharePct(latest!) === sharePct(previous!)) return 'the same as';
		return a > b ? 'up from' : 'down from';
	});

	const changes = $derived(weekly.map((p, i) => ({ p, i })).filter(({ p }) => judgeChanged(p)));
	const latestChanged = $derived(latest !== null && judgeChanged(latest));

	const placedMarkers = $derived(
		markers
			.map((m) => ({ m, i: weekIndexOf(weekly, m.date) }))
			.filter((e): e is { m: TrendMarker; i: number } => e.i !== null)
	);

	function markersIn(i: number): TrendMarker[] {
		return placedMarkers.filter((e) => e.i === i).map((e) => e.m);
	}

	function hitLeft(i: number): number {
		return i === 0 ? -M.left : (x(i - 1) + x(i)) / 2;
	}

	function hitRight(i: number): number {
		return i === weekly.length - 1 ? plotW + M.right : (x(i) + x(i + 1)) / 2;
	}

	function pointLabel(p: TrendPoint): string {
		if (p.n === 0) return `${weekLabel(p.weekEnd)}: no turns labelled.`;
		return `${weekLabel(p.weekEnd)}: ${sharePct(p)} labelled Corrected, ${p.k} of ${p.n} turns.`;
	}

	function onPointer(event: PointerEvent): void {
		const svg = event.currentTarget as SVGSVGElement;
		const box = svg.getBoundingClientRect();
		const px = event.clientX - box.left - M.left;
		let best = 0;
		for (let i = 1; i < weekly.length; i++) {
			if (Math.abs(x(i) - px) < Math.abs(x(best) - px)) best = i;
		}
		active = weekly.length > 0 ? best : null;
	}

	const activePoint = $derived(active === null ? null : (weekly[active] ?? null));
	const tipSide = $derived(active !== null && x(active) > plotW / 2 ? 'left' : 'right');
</script>

<section
	class="ct edge mech-group"
	aria-label="Weekly corrected share trend"
	data-slot="correction-trend"
>
	<div class="band-hd band-hd--static">
		<h2 class="machine ct-title">Corrected turns, {windowLabel}</h2>
	</div>
	<div class="band-body">
		{#if error}
			<LoadErrorBanner title="Could not load the trend." detail={error} {onRetry} />
		{:else if labelled.length === 0}
			<p class="ct-empty">Not enough judged turns yet to show a trend.</p>
		{:else}
			<p class="ct-scope" data-testid="ct-scope">
				Each week counts every real turn the judge labelled in those seven days, in every project,
				whether or not you replied. The
				<a class="ct-link" href={resolve('/sessions/labelling')}>Judge page</a> counts the same turns
				the same way, so its "last 7 days" figure matches the latest week here.
			</p>
			{#if latest}
				<p class="ct-latest" data-testid="ct-latest">
					{weekLabel(latest.weekEnd)}: {sharePct(latest)} of turns labelled Corrected ({latest.k} of
					{latest.n}){#if previous}, {directionWord}
						{sharePct(previous)} in {weekLabel(previous.weekEnd)}{/if}.
				</p>
			{/if}
			{#if changes.length > 0}
				<div class="ct-warn" role="note" data-testid="ct-judge-changed">
					<p class="ct-warn-lead">
						<span class="ct-warn-tag">Judge changed</span>
						The judge changed during these weeks. A different judge can label the same kind of turn differently,
						so a rise or fall across a change may come from the judge, not from Claude.{#if latestChanged}{' '}The
							latest week is one of them, so compare it with the week before with care.{/if}
					</p>
					<ul class="ct-warn-list">
						{#each changes as { p } (p.weekEnd)}
							<li>{weekLabel(p.weekEnd)}: {changeSentence(p)}</li>
						{/each}
					</ul>
				</div>
			{/if}
			<div class="ct-plot" bind:clientWidth={width}>
				<svg
					class="ct-svg"
					width={plotW + M.left + M.right}
					height={H}
					role="group"
					aria-label="Weekly share of turns labelled Corrected, {windowLabel}"
					onpointermove={onPointer}
					onpointerleave={() => (active = null)}
				>
					<g transform="translate({M.left},{M.top})">
						{#each ticks as t (t)}
							<line class="ct-grid" x1="0" x2={plotW} y1={y(t)} y2={y(t)} />
							<text class="ct-axis" x="-8" y={y(t)} dy="0.32em" text-anchor="end">{t}%</text>
						{/each}
						{#each xTicks as i (i)}
							<text class="ct-axis" x={x(i)} y={plotH + 18} text-anchor="middle"
								>{lastDayLabel(weekly[i]!.weekEnd)}</text
							>
						{/each}
						{#each changes as { p, i } (p.weekEnd)}
							{@const cx = i === 0 ? x(0) : (x(i - 1) + x(i)) / 2}
							<line
								class="ct-change"
								x1={cx}
								x2={cx}
								y1="0"
								y2={plotH}
								data-testid="ct-change-line"
							/>
						{/each}
						{#if activePoint && active !== null}
							<line class="ct-cross" x1={x(active)} x2={x(active)} y1="0" y2={plotH} />
						{/if}
						<path class="ct-line" d={path} />
						{#each weekly as p, i (p.weekEnd)}
							{@const v = shareValue(p)}
							{#if v !== null}
								<circle
									class="ct-dot"
									class:ct-dot--active={active === i}
									cx={x(i)}
									cy={y(v)}
									r="4"
								/>
							{/if}
						{/each}
						{#if latest}
							<text class="ct-end" x={x(lastIndex) + 8} y={y(shareValue(latest) ?? 0)} dy="0.32em"
								>{sharePct(latest)}</text
							>
						{/if}
						{#each placedMarkers as { m, i } (m.date + m.href)}
							<rect
								class="ct-decision"
								x={x(i) - 4}
								y={plotH - 4}
								width="8"
								height="8"
								transform="rotate(45 {x(i)} {plotH})"
							/>
						{/each}
						{#each weekly as p, i (p.weekEnd)}
							<!-- svelte-ignore a11y_no_noninteractive_tabindex -- each week takes keyboard focus so keyboard readers get the same tooltip pointer readers get; the table below carries the same values -->
							<rect
								class="ct-hit"
								x={hitLeft(i)}
								y={-M.top}
								width={Math.max(1, hitRight(i) - hitLeft(i))}
								height={H}
								tabindex="0"
								role="img"
								aria-label={pointLabel(p)}
								data-testid="ct-week"
								onfocus={() => (active = i)}
								onblur={() => (active = null)}
							/>
						{/each}
					</g>
				</svg>
				{#if activePoint && active !== null}
					<div
						class="ct-tip"
						class:ct-tip--left={tipSide === 'left'}
						style:left="{M.left + x(active)}px"
						role="status"
						data-testid="ct-tooltip"
					>
						<div class="ct-tip-when">{weekLabel(activePoint.weekEnd)}</div>
						{#if activePoint.n > 0}
							<div class="ct-tip-value">
								<span class="ct-key" aria-hidden="true"></span><strong
									>{sharePct(activePoint)}</strong
								>
								labelled Corrected
							</div>
							<div>{activePoint.k} of {activePoint.n} turns</div>
							{#if activePoint.judges.length > 0}
								<div class="ct-tip-sub">
									Judge: {activePoint.judges.map((j) => `${j.name} (${j.count})`).join(', ')}
								</div>
							{/if}
							{#if judgeChanged(activePoint)}
								<div class="ct-tip-sub">Judge changed: {changeSentence(activePoint)}</div>
							{/if}
						{:else}
							<div>No turns labelled</div>
						{/if}
						{#each markersIn(active) as m (m.date + m.href)}
							<div class="ct-tip-sub">{m.label}</div>
						{/each}
					</div>
				{/if}
			</div>
			<p class="ct-legend">
				Dots are weeks, dated by their last day.{#if changes.length > 0}{' '}A thin vertical line
					marks a change of judge.{/if}{#if placedMarkers.length > 0}{' '}A grey diamond on the
					bottom line marks a lesson or directive decision.{/if}
			</p>
			<details class="ct-table-wrap">
				<summary>Show the numbers</summary>
				<table class="ct-table">
					<thead>
						<tr
							><th scope="col">Week</th><th scope="col">Labelled Corrected</th><th scope="col"
								>Turns labelled</th
							><th scope="col">Judge</th></tr
						>
					</thead>
					<tbody>
						{#each weekly as p (p.weekEnd)}
							<tr>
								<td>{weekLabel(p.weekEnd)}</td>
								<td class="ct-num">{p.n > 0 ? `${sharePct(p)} (${p.k})` : '—'}</td>
								<td class="ct-num">{p.n}</td>
								<td>
									{p.judges.map((j) => j.name).join(', ') || '—'}{#if judgeChanged(p)}{' '}<span
											class="ct-changed-note">· changed</span
										>{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</details>
			{#if markers.length > 0}
				<ul class="ct-markers">
					{#each markers as marker (marker.date + marker.href)}
						<li>
							{formatDay(marker.date) ?? marker.date} —
							<a class="ct-link" href={marker.href}>{marker.label}</a>
						</li>
					{/each}
				</ul>
			{/if}
		{/if}
	</div>
</section>

<style>
	.ct {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.ct-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.ct-empty,
	.ct-scope {
		margin: 0 0 var(--space-2);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.ct-latest {
		margin: 0 0 var(--space-3);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.ct-warn {
		margin: 0 0 var(--space-3);
		padding: var(--space-2) var(--space-3);
		border-left: 2px solid var(--status-warning-border);
		background: var(--status-warning-bg);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.ct-warn-lead {
		margin: 0;
	}
	.ct-warn-tag {
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		text-transform: uppercase;
		color: var(--status-warning-fg);
		margin-right: var(--space-1);
	}
	.ct-warn-list {
		margin: var(--space-1) 0 0;
		padding-left: var(--space-4);
		color: var(--text-2);
	}
	.ct-plot {
		position: relative;
		width: 100%;
		min-width: 0;
	}
	.ct-svg {
		display: block;
		overflow: visible;
		touch-action: pan-y;
	}
	.ct-grid {
		stroke: var(--border-subtle);
		stroke-width: 1;
		shape-rendering: crispEdges;
	}
	.ct-axis {
		fill: var(--text-3);
		font-family: var(--font-mono);
		font-size: 11px;
		font-variant-numeric: tabular-nums;
	}
	.ct-end {
		fill: var(--text-1);
		font-family: var(--font-mono);
		font-size: 12px;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}
	.ct-change {
		stroke: var(--border-strong);
		stroke-width: 1;
		shape-rendering: crispEdges;
	}
	.ct-cross {
		stroke: var(--text-3);
		stroke-width: 1;
		shape-rendering: crispEdges;
		pointer-events: none;
	}
	.ct-line {
		fill: none;
		stroke: var(--data-teal-fg);
		stroke-width: 2;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.ct-dot {
		fill: var(--data-teal-fg);
		stroke: var(--surface-card);
		stroke-width: 2;
	}
	.ct-dot--active {
		r: 6;
	}
	.ct-decision {
		fill: var(--text-3);
		stroke: var(--surface-card);
		stroke-width: 2;
	}
	.ct-hit {
		fill: transparent;
		outline: none;
	}
	.ct-hit:focus-visible {
		stroke: var(--interactive-focus-ring);
		stroke-width: 2;
	}
	.ct-tip {
		position: absolute;
		top: 0;
		transform: translateX(12px);
		z-index: 2;
		min-width: 180px;
		max-width: 280px;
		padding: var(--space-2) var(--space-3);
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		box-shadow: 0 4px 12px rgb(0 0 0 / 0.25);
		font-size: var(--text-xs);
		color: var(--text-2);
		pointer-events: none;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.ct-tip--left {
		transform: translateX(calc(-100% - 12px));
	}
	.ct-tip-when {
		color: var(--text-1);
	}
	.ct-tip-value {
		display: flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--text-2);
	}
	.ct-tip-value strong {
		color: var(--text-strong);
		font-size: var(--text-sm);
		font-variant-numeric: tabular-nums;
	}
	.ct-key {
		display: inline-block;
		width: 12px;
		height: 2px;
		background: var(--data-teal-fg);
	}
	.ct-tip-sub {
		color: var(--text-3);
	}
	.ct-legend {
		margin: var(--space-2) 0 0;
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.ct-table-wrap {
		margin-top: var(--space-2);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.ct-table-wrap summary {
		cursor: pointer;
		color: var(--interactive-active-fg);
	}
	.ct-table-wrap summary:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ct-table {
		margin-top: var(--space-2);
		border-collapse: collapse;
		width: 100%;
	}
	.ct-table th,
	.ct-table td {
		text-align: left;
		padding: var(--space-1) var(--space-2);
		border-bottom: 1px solid var(--border-subtle);
	}
	.ct-table th {
		color: var(--text-3);
		font-weight: var(--weight-body);
	}
	.ct-num {
		font-variant-numeric: tabular-nums;
	}
	.ct-changed-note {
		color: var(--text-3);
	}
	.ct-markers {
		margin: var(--space-3) 0 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.ct-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.ct-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
