<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import Sparkline from './Sparkline.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Home/Sparkline',
		component: Sparkline,
		tags: ['autodocs'],
		argTypes: {
			stroke: { control: 'color' },
			kpi: { control: 'boolean' },
			values: { control: false }
		}
	});

	const ascendingValues = [1, 2, 3, 5, 8, 13, 21, 34, 55, 89];
	const descendingValues = [89, 55, 34, 21, 13, 8, 5, 3, 2, 1];
	const flatValues = [5, 5, 5, 5, 5];
	const singleValue = [42];
	const wideRangeValues = [0.001, 1000];
	const volatileValues = [5, 80, 10, 95, 3, 70, 45, 20, 88, 12, 60];
</script>

<Story name="EmptySeries" args={{ values: [], stroke: 'var(--status-muted-fg)' }}>
	{#snippet template(args)}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; align-items: center; gap: 8px; min-height: 40px;"
		>
			<Sparkline {...args} />
			<span style="font-size: 11px; color: var(--text-3);">(empty array — no SVG rendered)</span>
		</div>
	{/snippet}
</Story>

<Story name="SingleValue" args={{ values: singleValue, stroke: 'var(--status-warning-fg)' }}>
	{#snippet template(args)}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; align-items: center; gap: 8px;"
		>
			<Sparkline {...args} />
			<span style="font-size: 11px; color: var(--text-3);">1 sample — horizontal mid-line</span>
		</div>
	{/snippet}
</Story>

<Story name="Ascending" args={{ values: ascendingValues, stroke: 'var(--status-success-fg)' }}>
	{#snippet template(args)}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; align-items: center; gap: 8px;"
		>
			<Sparkline {...args} />
			<span style="font-size: 11px; color: var(--text-3);">rising trend, 10 samples</span>
		</div>
	{/snippet}
</Story>

<Story name="Descending" args={{ values: descendingValues, stroke: 'var(--status-danger-fg)' }}>
	{#snippet template(args)}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; align-items: center; gap: 8px;"
		>
			<Sparkline {...args} />
			<span style="font-size: 11px; color: var(--text-3);">falling trend, 10 samples</span>
		</div>
	{/snippet}
</Story>

<Story name="FlatSeries" args={{ values: flatValues, stroke: 'var(--status-info-fg)' }}>
	{#snippet template(args)}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; align-items: center; gap: 8px;"
		>
			<Sparkline {...args} />
			<span style="font-size: 11px; color: var(--text-3);"
				>flat — all values equal, sits at y=20 (bottom edge)</span
			>
		</div>
	{/snippet}
</Story>

<Story name="WideRange" args={{ values: wideRangeValues, stroke: 'var(--data-viz-5-fg)' }}>
	{#snippet template(args)}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; align-items: center; gap: 8px;"
		>
			<Sparkline {...args} />
			<span style="font-size: 11px; color: var(--text-3);"
				>[0.001, 1000] — wide absolute scale, shape fills height</span
			>
		</div>
	{/snippet}
</Story>

<Story
	name="NormalVariant"
	args={{ values: ascendingValues, stroke: 'var(--agent-2-fg)', kpi: false }}
>
	{#snippet template(args)}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; align-items: center; gap: 8px;"
		>
			<Sparkline {...args} />
			<span style="font-size: 11px; color: var(--text-3);">kpi=false — 60×20px SVG</span>
		</div>
	{/snippet}
</Story>

<Story
	name="KpiVariant"
	args={{ values: ascendingValues, stroke: 'var(--data-viz-5-fg)', kpi: true }}
>
	{#snippet template(args)}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; align-items: center; gap: 8px;"
		>
			<Sparkline {...args} />
			<span style="font-size: 11px; color: var(--text-3);"
				>kpi=true — 60×16px with margin-top 4px</span
			>
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemes"
	tags={['design-exploration']}
	args={{ values: ascendingValues, stroke: 'var(--status-success-fg)' }}
>
	{#snippet template(args)}
		<div style="display: flex; flex-direction: column; gap: 2px; font-size: 11px;">
			<div
				data-theme="forge"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">forge</span>
				<Sparkline {...args} />
				<Sparkline values={volatileValues} stroke="var(--data-viz-5-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--agent-2-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
			</div>
			<div
				data-theme="glacier"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">glacier</span>
				<Sparkline {...args} />
				<Sparkline values={volatileValues} stroke="var(--data-viz-5-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--agent-2-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
			</div>
			<div
				data-theme="void"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">void</span>
				<Sparkline {...args} />
				<Sparkline values={volatileValues} stroke="var(--data-viz-5-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--agent-2-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
			</div>
			<div
				data-theme="phosphor"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">phosphor</span>
				<Sparkline {...args} />
				<Sparkline values={volatileValues} stroke="var(--data-viz-5-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--agent-2-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
			</div>
			<div
				data-theme="crimson"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">crimson</span>
				<Sparkline {...args} />
				<Sparkline values={volatileValues} stroke="var(--data-viz-5-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--agent-2-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="EmptyInteraction"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const svg = canvasElement.querySelector('svg');
		await expect(svg, 'SVG must NOT be rendered when values is empty').toBeNull();
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface-base); min-height: 40px;">
			<Sparkline values={[]} stroke="var(--status-muted-fg)" />
		</div>
	{/snippet}
</Story>

<Story
	name="PolylinePoints"
	tags={['interactive']}
	args={{ values: ascendingValues, stroke: 'var(--status-success-fg)' }}
	play={async ({ canvasElement, args }) => {
		const svg = canvasElement.querySelector('svg');
		await expect(svg, 'SVG must be rendered when values is non-empty').toBeTruthy();
		await expect(svg?.getAttribute('aria-hidden'), 'aria-hidden must be true').toBe('true');
		await expect(svg?.getAttribute('role'), 'role must be presentation').toBe('presentation');

		const polyline = svg?.querySelector('polyline');
		await expect(polyline, 'polyline element must be present').toBeTruthy();

		const points = polyline?.getAttribute('points') ?? '';
		const pairs = points.trim().split(/\s+/);
		await expect(pairs.length, 'points must contain one pair per value (10 items = 10 pairs)').toBe(
			10
		);
		const firstPair = pairs[0];
		await expect(firstPair, 'first point must be "0.00,20.00" (min value at x=0, y=bottom)').toBe(
			'0.00,20.00'
		);

		await expect(
			polyline?.getAttribute('vector-effect'),
			'vector-effect must be non-scaling-stroke'
		).toBe('non-scaling-stroke');
		await expect(polyline?.getAttribute('fill'), 'fill must be none').toBe('none');
		await expect(polyline?.getAttribute('stroke'), 'stroke must equal args.stroke').toBe(
			args.stroke
		);
	}}
>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface-base);">
			<Sparkline {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="FlatLineCenter"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const polyline = canvasElement.querySelector('polyline');
		await expect(polyline, 'polyline must be present for flat series').toBeTruthy();

		const points = polyline?.getAttribute('points') ?? '';
		const pairs = points.trim().split(/\s+/);
		await expect(pairs.length, 'points must contain one pair per value (5 items = 5 pairs)').toBe(
			5
		);

		const yValues = pairs.map((pair) => pair.split(',')[1]);
		for (const y of yValues) {
			await expect(y, 'each y value must be "20.00" (bottom edge, v-min=0)').toBe('20.00');
		}
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface-base);">
			<Sparkline values={flatValues} stroke="var(--status-info-fg)" />
		</div>
	{/snippet}
</Story>

<Story
	name="SingleValueLine"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const polyline = canvasElement.querySelector('polyline');
		await expect(polyline, 'polyline must be present for single-value series').toBeTruthy();

		const points = polyline?.getAttribute('points') ?? '';
		await expect(points, 'single-value points must be the horizontal mid-line').toBe('0,10 60,10');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface-base);">
			<Sparkline values={singleValue} stroke="var(--status-warning-fg)" />
		</div>
	{/snippet}
</Story>

<Story
	name="KpiDimensions"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const normalSvg = canvasElement.querySelector('[data-variant="normal"] svg');
		const kpiSvg = canvasElement.querySelector('[data-variant="kpi"] svg');

		await expect(normalSvg, 'normal SVG must be present').toBeTruthy();
		await expect(kpiSvg, 'kpi SVG must be present').toBeTruthy();

		await waitFor(() => {
			const normalRect = (normalSvg as Element).getBoundingClientRect();
			expect(normalRect.width, 'normal SVG width must be 60px').toBe(60);
			expect(normalRect.height, 'normal SVG height must be 20px').toBe(20);
		});

		await waitFor(() => {
			const kpiRect = (kpiSvg as Element).getBoundingClientRect();
			expect(kpiRect.width, 'kpi SVG width must be 60px').toBe(60);
			expect(kpiRect.height, 'kpi SVG height must be 16px').toBe(16);
		});
	}}
>
	{#snippet template()}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; gap: 16px; align-items: flex-start;"
		>
			<div data-variant="normal">
				<Sparkline values={ascendingValues} stroke="var(--agent-2-fg)" kpi={false} />
			</div>
			<div data-variant="kpi">
				<Sparkline values={ascendingValues} stroke="var(--data-viz-5-fg)" kpi={true} />
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="VolatileCoordinates"
	tags={['interactive']}
	args={{ values: volatileValues, stroke: 'var(--data-viz-5-fg)' }}
	play={async ({ canvasElement, args }) => {
		const polyline = canvasElement.querySelector('polyline');
		await expect(polyline, 'polyline must be present for volatile series').toBeTruthy();

		const points = polyline?.getAttribute('points') ?? '';
		const pairs = points.trim().split(/\s+/);
		await expect(pairs.length, 'one coordinate pair per value (11 values = 11 pairs)').toBe(11);

		const xValues = pairs.map((p) => parseFloat(p.split(',')[0]!));
		const yValues = pairs.map((p) => parseFloat(p.split(',')[1]!));

		await expect(xValues[0], 'first x must be 0').toBe(0);
		await expect(xValues[xValues.length - 1], 'last x must be 60').toBe(60);

		for (let i = 1; i < xValues.length; i++) {
			await expect(xValues[i]!, `x[${i}] must be greater than x[${i - 1}]`).toBeGreaterThan(
				xValues[i - 1]!
			);
		}

		const distinctY = new Set(yValues).size;
		await expect(
			distinctY,
			'volatile series must have more than 1 distinct y value (non-monotone shape)'
		).toBeGreaterThan(1);

		const minY = Math.min(...yValues);
		await expect(minY, 'minimum y must be 0.00 (highest input value maps to top)').toBe(0);

		const maxY = Math.max(...yValues);
		await expect(maxY, 'maximum y must be 20.00 (lowest input value maps to bottom)').toBe(20);

		await expect(polyline?.getAttribute('stroke'), 'stroke must match args.stroke').toBe(
			args.stroke
		);
	}}
>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface-base);">
			<Sparkline {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Snapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'Sparkline/AllStates');
	}}
>
	{#snippet template()}
		<div
			style="padding: 16px; background: var(--surface-base); display: flex; flex-direction: column; gap: 8px; font-size: 11px; color: var(--text-3);"
		>
			<div style="display: flex; align-items: center; gap: 12px;">
				<span style="width: 120px;">ascending</span>
				<Sparkline values={ascendingValues} stroke="var(--status-success-fg)" />
			</div>
			<div style="display: flex; align-items: center; gap: 12px;">
				<span style="width: 120px;">descending</span>
				<Sparkline values={descendingValues} stroke="var(--status-danger-fg)" />
			</div>
			<div style="display: flex; align-items: center; gap: 12px;">
				<span style="width: 120px;">flat</span>
				<Sparkline values={flatValues} stroke="var(--status-info-fg)" />
			</div>
			<div style="display: flex; align-items: center; gap: 12px;">
				<span style="width: 120px;">single value</span>
				<Sparkline values={singleValue} stroke="var(--status-warning-fg)" />
			</div>
			<div style="display: flex; align-items: center; gap: 12px;">
				<span style="width: 120px;">wide range</span>
				<Sparkline values={wideRangeValues} stroke="var(--data-viz-5-fg)" />
			</div>
			<div style="display: flex; align-items: center; gap: 12px;">
				<span style="width: 120px;">kpi (16px)</span>
				<Sparkline values={ascendingValues} stroke="var(--agent-2-fg)" kpi={true} />
			</div>
			<div style="display: flex; align-items: center; gap: 12px;">
				<span style="width: 120px;">normal (20px)</span>
				<Sparkline values={ascendingValues} stroke="var(--agent-2-fg)" kpi={false} />
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'Sparkline/AllThemes');
	}}
>
	{#snippet template()}
		<div style="display: flex; flex-direction: column; gap: 2px; font-size: 11px;">
			<div
				data-theme="forge"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">forge</span>
				<Sparkline values={ascendingValues} stroke="var(--status-success-fg)" />
				<Sparkline values={descendingValues} stroke="var(--status-danger-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--data-viz-5-fg)" kpi={true} />
			</div>
			<div
				data-theme="glacier"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">glacier</span>
				<Sparkline values={ascendingValues} stroke="var(--status-success-fg)" />
				<Sparkline values={descendingValues} stroke="var(--status-danger-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--data-viz-5-fg)" kpi={true} />
			</div>
			<div
				data-theme="void"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">void</span>
				<Sparkline values={ascendingValues} stroke="var(--status-success-fg)" />
				<Sparkline values={descendingValues} stroke="var(--status-danger-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--data-viz-5-fg)" kpi={true} />
			</div>
			<div
				data-theme="phosphor"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">phosphor</span>
				<Sparkline values={ascendingValues} stroke="var(--status-success-fg)" />
				<Sparkline values={descendingValues} stroke="var(--status-danger-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--data-viz-5-fg)" kpi={true} />
			</div>
			<div
				data-theme="crimson"
				style="padding: 8px 16px; background: var(--surface-base); display: flex; align-items: center; gap: 12px;"
			>
				<span style="color: var(--text-3); width: 80px;">crimson</span>
				<Sparkline values={ascendingValues} stroke="var(--status-success-fg)" />
				<Sparkline values={descendingValues} stroke="var(--status-danger-fg)" />
				<Sparkline values={flatValues} stroke="var(--interactive-active-fg)" />
				<Sparkline values={ascendingValues} stroke="var(--data-viz-5-fg)" kpi={true} />
			</div>
		</div>
	{/snippet}
</Story>
