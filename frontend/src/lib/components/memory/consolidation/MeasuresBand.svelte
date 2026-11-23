<script lang="ts">
	import { TrendChart, formatRun } from '$lib/components/ui/trend-chart';
	import type { MeasureSeriesRow } from '$lib/api/knowledgeConsolidation';
	import { MEASURES } from './labels';

	interface Props {
		measures: MeasureSeriesRow[];
		/** Epoch ms of the run pointed at, shared by all eight charts. */
		pointAt?: number | null;
	}

	let { measures, pointAt = $bindable(null) }: Props = $props();

	const times = $derived(measures.map((m) => Date.parse(m.measured_at)));
	const domain = $derived<[number, number]>(
		times.length > 0 ? [times[0]!, times[times.length - 1]!] : [0, 1]
	);

	function seriesOf(key: keyof MeasureSeriesRow) {
		return measures.map((m) => ({ at: m.measured_at, value: Number(m[key]) }));
	}

	const pointedRow = $derived(
		pointAt === null ? null : (measures.find((m) => Date.parse(m.measured_at) === pointAt) ?? null)
	);
	const announcement = $derived(
		pointedRow
			? `${formatRun(pointAt!)}: ${MEASURES.map(
					(def) => `${def.label.toLowerCase()} ${def.format(Number(pointedRow[def.key]))}`
				).join(', ')}`
			: ''
	);

	function targetIndex(key: string, at: number): number | null {
		if (key === 'ArrowLeft') return pointAt === null ? at : Math.max(0, at - 1);
		if (key === 'ArrowRight') return pointAt === null ? at : Math.min(times.length - 1, at + 1);
		if (key === 'Home') return 0;
		if (key === 'End') return times.length - 1;
		return null;
	}

	function onkeydown(event: KeyboardEvent): void {
		if (times.length === 0) return;
		if (event.key === 'Escape') {
			pointAt = null;
			return;
		}
		const next = targetIndex(
			event.key,
			pointAt === null ? times.length - 1 : times.indexOf(pointAt)
		);
		if (next === null) return;
		event.preventDefault();
		pointAt = times[next]!;
	}

	function onfocus(): void {
		if (pointAt === null && times.length > 0) pointAt = times[times.length - 1]!;
	}
</script>

<!-- the chart grid is one keyboard tab stop that steps between runs with the arrow keys -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
	class="mb"
	role="group"
	tabindex="0"
	aria-label="Measures by run. Left and right arrows move between runs."
	data-slot="measures-band"
	{onkeydown}
	{onfocus}
>
	<div class="mb__grid">
		{#each MEASURES as def (def.key)}
			<div class="mb__cell">
				<TrendChart
					label={def.label}
					tooltip={def.tooltip}
					points={seriesOf(def.key)}
					format={def.format}
					formatDelta={def.formatDelta}
					better={def.better}
					{domain}
					bind:pointAt
				/>
			</div>
		{/each}
	</div>
	<span class="sr-only" aria-live="polite">{announcement}</span>
</div>

<style>
	.mb {
		container-type: inline-size;
	}
	.mb__grid {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: var(--hairline);
		background: var(--border-subtle);
	}
	.mb:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.mb__cell {
		padding: var(--space-3);
		background: var(--surface-card);
		min-width: 0;
	}
	@container (max-width: 52rem) {
		.mb__grid {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
	@container (max-width: 30rem) {
		.mb__grid {
			grid-template-columns: minmax(0, 1fr);
		}
	}
</style>
