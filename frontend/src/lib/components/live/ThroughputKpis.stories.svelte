<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import ThroughputKpis from './ThroughputKpis.svelte';
	import type { ThroughputSnapshot } from '$lib/state/liveSystem.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Home/ThroughputKpis',
		component: ThroughputKpis,
		tags: ['autodocs'],
		argTypes: {
			throughput: { control: false },
			stale: { control: 'boolean' }
		}
	});

	const historyRising = [1, 2, 3, 4, 5, 6, 7];
	const historyFlat = [4, 4, 4, 4, 4];
	const historyBusy = [10, 18, 22, 30, 28, 35, 40, 38, 45, 50];
	const historySingle = [42];

	const idle: ThroughputSnapshot = {
		req_per_min: 0,
		tokens_per_sec: 0,
		error_rate_pct: 0,
		req_history: [],
		tokens_history: []
	};

	const normal: ThroughputSnapshot = {
		req_per_min: 4.2,
		tokens_per_sec: 1840,
		error_rate_pct: 0,
		req_history: historyRising,
		tokens_history: historyRising
	};

	const busy: ThroughputSnapshot = {
		req_per_min: 28.6,
		tokens_per_sec: 12_400,
		error_rate_pct: 0,
		req_history: historyBusy,
		tokens_history: historyBusy
	};

	const lowErrors: ThroughputSnapshot = {
		req_per_min: 8.1,
		tokens_per_sec: 3_200,
		error_rate_pct: 4.5,
		req_history: historyFlat,
		tokens_history: historyFlat
	};

	const highErrors: ThroughputSnapshot = {
		req_per_min: 6.0,
		tokens_per_sec: 1_100,
		error_rate_pct: 15.3,
		req_history: historyFlat,
		tokens_history: historyFlat
	};

	const errBoundary: ThroughputSnapshot = {
		req_per_min: 6.0,
		tokens_per_sec: 1_100,
		error_rate_pct: 10.0,
		req_history: historyFlat,
		tokens_history: historyFlat
	};

	const emptySparklines: ThroughputSnapshot = {
		req_per_min: 4.2,
		tokens_per_sec: 1840,
		error_rate_pct: 0,
		req_history: [],
		tokens_history: []
	};

	const singleValueSparklines: ThroughputSnapshot = {
		req_per_min: 4.2,
		tokens_per_sec: 1840,
		error_rate_pct: 0,
		req_history: historySingle,
		tokens_history: historySingle
	};
</script>

<Story name="Idle" args={{ throughput: idle }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Normal" args={{ throughput: normal }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Busy" args={{ throughput: busy }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis {...args} />
		</div>
	{/snippet}
</Story>

<Story name="LowErrors" args={{ throughput: lowErrors }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis {...args} />
		</div>
	{/snippet}
</Story>

<Story name="HighErrors" args={{ throughput: highErrors }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Stale" args={{ throughput: normal, stale: true }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis {...args} />
		</div>
	{/snippet}
</Story>

<Story name="EmptySparklines" args={{ throughput: emptySparklines }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis {...args} />
		</div>
	{/snippet}
</Story>

<Story name="SingleValueSparklines" args={{ throughput: singleValueSparklines }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="IdleSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/Idle');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={idle} />
		</div>
	{/snippet}
</Story>

<Story
	name="NormalSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/Normal');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={normal} />
		</div>
	{/snippet}
</Story>

<Story
	name="BusySnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/Busy');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={busy} />
		</div>
	{/snippet}
</Story>

<Story
	name="LowErrorsSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/LowErrors');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={lowErrors} />
		</div>
	{/snippet}
</Story>

<Story
	name="HighErrorsSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/HighErrors');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={highErrors} />
		</div>
	{/snippet}
</Story>

<Story
	name="StaleSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/Stale');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={normal} stale />
		</div>
	{/snippet}
</Story>

<Story
	name="AllStatesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/AllStates');
	}}
>
	{#snippet template()}
		<div
			style="padding: 16px; background: var(--surface, #161b22); display: flex; flex-direction: column; gap: 24px;"
		>
			<div>
				<p
					style="font-size: 10px; color: var(--text-3, #c0c7d0); margin: 0 0 8px; text-transform: uppercase; letter-spacing: .06em;"
				>
					Idle
				</p>
				<ThroughputKpis throughput={idle} />
			</div>
			<div>
				<p
					style="font-size: 10px; color: var(--text-3, #c0c7d0); margin: 0 0 8px; text-transform: uppercase; letter-spacing: .06em;"
				>
					Normal (0% errors — green)
				</p>
				<ThroughputKpis throughput={normal} />
			</div>
			<div>
				<p
					style="font-size: 10px; color: var(--text-3, #c0c7d0); margin: 0 0 8px; text-transform: uppercase; letter-spacing: .06em;"
				>
					Busy
				</p>
				<ThroughputKpis throughput={busy} />
			</div>
			<div>
				<p
					style="font-size: 10px; color: var(--text-3, #c0c7d0); margin: 0 0 8px; text-transform: uppercase; letter-spacing: .06em;"
				>
					Low errors (4.5% — amber)
				</p>
				<ThroughputKpis throughput={lowErrors} />
			</div>
			<div>
				<p
					style="font-size: 10px; color: var(--text-3, #c0c7d0); margin: 0 0 8px; text-transform: uppercase; letter-spacing: .06em;"
				>
					High errors (15.3% — red)
				</p>
				<ThroughputKpis throughput={highErrors} />
			</div>
			<div>
				<p
					style="font-size: 10px; color: var(--text-3, #c0c7d0); margin: 0 0 8px; text-transform: uppercase; letter-spacing: .06em;"
				>
					Stale (50% opacity)
				</p>
				<ThroughputKpis throughput={normal} stale />
			</div>
			<div>
				<p
					style="font-size: 10px; color: var(--text-3, #c0c7d0); margin: 0 0 8px; text-transform: uppercase; letter-spacing: .06em;"
				>
					Empty sparklines (no history)
				</p>
				<ThroughputKpis throughput={emptySparklines} />
			</div>
			<div>
				<p
					style="font-size: 10px; color: var(--text-3, #c0c7d0); margin: 0 0 8px; text-transform: uppercase; letter-spacing: .06em;"
				>
					Single-value sparklines (flat line)
				</p>
				<ThroughputKpis throughput={singleValueSparklines} />
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="EmptySparklinesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/EmptySparklines');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={emptySparklines} />
		</div>
	{/snippet}
</Story>

<Story
	name="SingleValueSparklinesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/SingleValueSparklines');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={singleValueSparklines} />
		</div>
	{/snippet}
</Story>

<Story
	name="ErrBoundarySnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ThroughputKpis/ErrBoundary');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={errBoundary} />
		</div>
	{/snippet}
</Story>

<Story
	name="ErrorRateClasses"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const rowEls = canvasElement.querySelectorAll<HTMLElement>('.lp-kpi-row');
		await expect(rowEls.length, 'three KPI rows should be rendered').toBe(3);

		const normalErrVal = within(rowEls[0]!).queryAllByText('0.0%')[0];
		await expect(normalErrVal, 'normal: error-rate val should exist').toBeTruthy();
		await expect(
			normalErrVal?.classList.contains('lp-kpi-val--ok'),
			'normal: --ok class present'
		).toBe(true);
		await expect(
			normalErrVal?.classList.contains('lp-kpi-val--warn'),
			'normal: --warn class absent'
		).toBe(false);
		await expect(
			normalErrVal?.classList.contains('lp-kpi-val--err'),
			'normal: --err class absent'
		).toBe(false);

		const lowErrVal = within(rowEls[1]!).queryAllByText('4.5%')[0];
		await expect(lowErrVal, 'low-errors: error-rate val should exist').toBeTruthy();
		await expect(
			lowErrVal?.classList.contains('lp-kpi-val--warn'),
			'low-errors: --warn class present'
		).toBe(true);
		await expect(
			lowErrVal?.classList.contains('lp-kpi-val--ok'),
			'low-errors: --ok class absent'
		).toBe(false);
		await expect(
			lowErrVal?.classList.contains('lp-kpi-val--err'),
			'low-errors: --err class absent'
		).toBe(false);

		const highErrVal = within(rowEls[2]!).queryAllByText('15.3%')[0];
		await expect(highErrVal, 'high-errors: error-rate val should exist').toBeTruthy();
		await expect(
			highErrVal?.classList.contains('lp-kpi-val--err'),
			'high-errors: --err class present'
		).toBe(true);
		await expect(
			highErrVal?.classList.contains('lp-kpi-val--ok'),
			'high-errors: --ok class absent'
		).toBe(false);
		await expect(
			highErrVal?.classList.contains('lp-kpi-val--warn'),
			'high-errors: --warn class absent'
		).toBe(false);

		await expect(
			canvas.queryAllByText('0.0%').length,
			'at least one 0.0% cell in canvas'
		).toBeGreaterThan(0);
	}}
>
	{#snippet template()}
		<div
			style="padding: 16px; background: var(--surface, #161b22); display: flex; flex-direction: column; gap: 24px;"
		>
			<ThroughputKpis throughput={normal} />
			<ThroughputKpis throughput={lowErrors} />
			<ThroughputKpis throughput={highErrors} />
		</div>
	{/snippet}
</Story>

<Story
	name="ErrorRateBoundary"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const errVal = canvas.queryAllByText('10.0%')[0];
		await expect(errVal, 'error-rate val should exist').toBeTruthy();
		await expect(
			errVal?.classList.contains('lp-kpi-val--err'),
			'error_rate_pct=10.0 must trigger --err (>= 10 threshold)'
		).toBe(true);
		await expect(
			errVal?.classList.contains('lp-kpi-val--warn'),
			'error_rate_pct=10.0 must NOT trigger --warn'
		).toBe(false);
		await expect(
			errVal?.classList.contains('lp-kpi-val--ok'),
			'error_rate_pct=10.0 must NOT trigger --ok'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={errBoundary} />
		</div>
	{/snippet}
</Story>

<Story
	name="NumberFormatting"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const reqVal = canvas.queryAllByText('4.2')[0];
		await expect(reqVal, 'req/min val should exist').toBeTruthy();
		await expect(reqVal?.textContent?.trim(), 'req/min: toFixed(1)').toBe('4.2');

		const tokVal = canvas.queryAllByText('1.8k')[0];
		await expect(tokVal, 'tok/sec val should exist').toBeTruthy();
		await expect(tokVal?.textContent?.trim(), 'tok/sec: /1000 toFixed(1) + k').toBe('1.8k');

		const errVal = canvas.queryAllByText('0.0%')[0];
		await expect(errVal, 'error-rate val should exist').toBeTruthy();
		await expect(errVal?.textContent?.trim(), 'error_rate_pct: toFixed(1) + %').toBe('0.0%');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={normal} />
		</div>
	{/snippet}
</Story>

<Story
	name="BusyNumberFormatting"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const reqVal = canvas.queryAllByText('28.6')[0];
		await expect(reqVal, 'busy req/min val should exist').toBeTruthy();
		await expect(reqVal?.textContent?.trim(), 'busy req/min: 28.6').toBe('28.6');

		const tokVal = canvas.queryAllByText('12.4k')[0];
		await expect(tokVal, 'busy tok/sec val should exist').toBeTruthy();
		await expect(tokVal?.textContent?.trim(), 'busy tok/sec: 12.4k').toBe('12.4k');

		const errVal = canvas.queryAllByText('0.0%')[0];
		await expect(errVal, 'busy error rate val should exist').toBeTruthy();
		await expect(errVal?.textContent?.trim(), 'busy error rate: 0.0%').toBe('0.0%');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={busy} />
		</div>
	{/snippet}
</Story>

<Story
	name="StaleClass"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const vals = canvasElement.querySelectorAll('.lp-kpi-val');
		await expect(vals.length, 'three .lp-kpi-val divs').toBe(3);
		for (let i = 0; i < vals.length; i++) {
			await expect(
				vals[i]?.classList.contains('lp-td-stale'),
				`lp-kpi-val[${i}] should have lp-td-stale`
			).toBe(true);
		}

		const labelTexts = ['req / min', 'tok / sec', 'error rate'];
		for (const text of labelTexts) {
			const label = canvas.queryAllByText(text)[0];
			await expect(label, `label "${text}" should exist`).toBeTruthy();
			await expect(
				label?.classList.contains('lp-td-stale'),
				`label "${text}" should NOT have lp-td-stale`
			).toBe(false);
		}

		const sparklines = canvasElement.querySelectorAll('.lp-sparkline');
		for (let i = 0; i < sparklines.length; i++) {
			await expect(
				sparklines[i]?.classList.contains('lp-td-stale'),
				`lp-sparkline[${i}] should NOT have lp-td-stale`
			).toBe(false);
		}
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={normal} stale />
		</div>
	{/snippet}
</Story>

<Story
	name="EmptySparklinesDontRender"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const svgs = canvasElement.querySelectorAll('svg.lp-sparkline');
		await expect(svgs.length, 'no sparkline SVGs should render with empty histories').toBe(0);

		const reqVal = canvas.queryAllByText('4.2')[0];
		await expect(reqVal, 'req/min value still renders').toBeTruthy();
		const tokVal = canvas.queryAllByText('1.8k')[0];
		await expect(tokVal, 'tok/sec value still renders').toBeTruthy();
		const errVal = canvas.queryAllByText('0.0%')[0];
		await expect(errVal, 'error-rate value still renders').toBeTruthy();
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={emptySparklines} />
		</div>
	{/snippet}
</Story>

<Story
	name="SingleValueSparklineRenders"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const svgs = canvasElement.querySelectorAll('svg.lp-sparkline');
		await expect(svgs.length, 'two sparkline SVGs should render with single-value histories').toBe(
			2
		);

		const polylines = canvasElement.querySelectorAll('svg.lp-sparkline polyline');
		await expect(polylines.length, 'two polylines').toBe(2);
		for (let i = 0; i < polylines.length; i++) {
			const pts = polylines[i]?.getAttribute('points');
			await expect(pts, `polyline[${i}] should be flat line`).toBe('0,10 60,10');
		}

		const reqVal = canvas.queryAllByText('4.2')[0];
		await expect(reqVal, 'req/min value still renders').toBeTruthy();
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<ThroughputKpis throughput={singleValueSparklines} />
		</div>
	{/snippet}
</Story>
