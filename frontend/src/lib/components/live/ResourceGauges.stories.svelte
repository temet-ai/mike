<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import ResourceGauges from './ResourceGauges.svelte';
	import type { MemorySnapshot } from '$lib/state/liveSystem.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Home/ResourceGauges',
		component: ResourceGauges,
		tags: ['autodocs'],
		argTypes: {
			memory: { control: false },
			stale: { control: 'boolean' }
		}
	});

	const MAX_STORE = 104_857_600;

	const ALL_7_FIELDS = [
		'request_body',
		'response_body',
		'filtered_request_body',
		'tool_calls',
		'tool_results',
		'tool_results_in_body',
		'system_prompt_blocks'
	];

	const breakdown7 = [
		{ field: 'request_body', bytes: 1_843_200 },
		{ field: 'response_body', bytes: 1_048_576 },
		{ field: 'filtered_request_body', bytes: 524_288 },
		{ field: 'tool_calls', bytes: 393_216 },
		{ field: 'tool_results', bytes: 262_144 },
		{ field: 'tool_results_in_body', bytes: 131_072 },
		{ field: 'system_prompt_blocks', bytes: 49_344 }
	];

	const historyLow = [3_100_000, 3_400_000, 3_800_000, 4_000_000, 4_100_000, 4_200_000, 4_251_840];

	const memLow: MemorySnapshot = {
		entries_total_bytes: 4_251_840,
		max_store_bytes: MAX_STORE,
		rss_bytes: 78_643_200,
		breakdown: breakdown7,
		history: historyLow
	};

	const memWarn: MemorySnapshot = {
		entries_total_bytes: 79_691_776,
		max_store_bytes: MAX_STORE,
		rss_bytes: 167_772_160,
		breakdown: [
			{ field: 'request_body', bytes: 32_000_000 },
			{ field: 'response_body', bytes: 22_000_000 },
			{ field: 'filtered_request_body', bytes: 10_000_000 },
			{ field: 'tool_calls', bytes: 8_000_000 },
			{ field: 'tool_results', bytes: 5_000_000 },
			{ field: 'tool_results_in_body', bytes: 2_000_000 },
			{ field: 'system_prompt_blocks', bytes: 691_776 }
		],
		history: [50_000_000, 60_000_000, 66_000_000, 71_000_000, 74_000_000, 77_000_000, 79_691_776]
	};

	const memCrit: MemorySnapshot = {
		entries_total_bytes: 98_566_144,
		max_store_bytes: MAX_STORE,
		rss_bytes: 218_103_808,
		breakdown: [
			{ field: 'request_body', bytes: 42_000_000 },
			{ field: 'response_body', bytes: 28_000_000 },
			{ field: 'filtered_request_body', bytes: 13_000_000 },
			{ field: 'tool_calls', bytes: 9_000_000 },
			{ field: 'tool_results', bytes: 4_000_000 },
			{ field: 'tool_results_in_body', bytes: 2_000_000 },
			{ field: 'system_prompt_blocks', bytes: 566_144 }
		],
		history: [80_000_000, 85_000_000, 90_000_000, 94_000_000, 96_000_000, 97_000_000, 98_566_144]
	};

	const memOver: MemorySnapshot = {
		entries_total_bytes: 120_586_240,
		max_store_bytes: MAX_STORE,
		rss_bytes: 268_435_456,
		breakdown: [
			{ field: 'request_body', bytes: 50_000_000 },
			{ field: 'response_body', bytes: 35_000_000 },
			{ field: 'filtered_request_body', bytes: 15_000_000 },
			{ field: 'tool_calls', bytes: 10_000_000 },
			{ field: 'tool_results', bytes: 7_000_000 },
			{ field: 'tool_results_in_body', bytes: 2_000_000 },
			{ field: 'system_prompt_blocks', bytes: 1_586_240 }
		],
		history: [
			90_000_000, 96_000_000, 100_000_000, 108_000_000, 112_000_000, 117_000_000, 120_586_240
		]
	};

	const memUnbudgeted: MemorySnapshot = {
		entries_total_bytes: 4_251_840,
		max_store_bytes: 0,
		rss_bytes: 78_643_200,
		breakdown: breakdown7,
		history: historyLow
	};

	const memRssNA: MemorySnapshot = {
		entries_total_bytes: 4_251_840,
		max_store_bytes: MAX_STORE,
		rss_bytes: 0,
		breakdown: breakdown7,
		history: historyLow
	};

	const memNoHistory: MemorySnapshot = {
		entries_total_bytes: 4_251_840,
		max_store_bytes: MAX_STORE,
		rss_bytes: 78_643_200,
		breakdown: breakdown7,
		history: []
	};

	const memEmptyField: MemorySnapshot = {
		entries_total_bytes: 8_388_608,
		max_store_bytes: MAX_STORE,
		rss_bytes: 78_643_200,
		breakdown: [
			{ field: 'request_body', bytes: 5_000_000 },
			{ field: 'response_body', bytes: 3_000_000 },
			{ field: 'filtered_request_body', bytes: 388_608 },
			{ field: 'tool_calls', bytes: 0 },
			{ field: 'tool_results', bytes: 0 },
			{ field: 'tool_results_in_body', bytes: 0 },
			{ field: 'system_prompt_blocks', bytes: 0 }
		],
		history: [7_000_000, 8_388_608]
	};

	const memUnknownField: MemorySnapshot = {
		entries_total_bytes: 8_388_608,
		max_store_bytes: MAX_STORE,
		rss_bytes: 78_643_200,
		breakdown: [
			{ field: 'request_body', bytes: 4_000_000 },
			{ field: 'response_body', bytes: 2_000_000 },
			{ field: 'unknown_future_field', bytes: 1_500_000 },
			{ field: 'another_unknown_field', bytes: 888_608 }
		],
		history: [7_000_000, 8_388_608]
	};
</script>

<Story name="LowUsage" tags={['data-driven']} args={{ memory: memLow }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="WarnUsage" tags={['data-driven']} args={{ memory: memWarn }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="CritUsage" tags={['data-driven']} args={{ memory: memCrit }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="OverBudget" tags={['data-driven']} args={{ memory: memOver }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Unbudgeted" tags={['data-driven']} args={{ memory: memUnbudgeted }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="RssNA" tags={['data-driven']} args={{ memory: memRssNA }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="HasHistory" tags={['data-driven']} args={{ memory: memLow }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="NoHistory" tags={['data-driven']} args={{ memory: memNoHistory }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Stale" tags={['data-driven']} args={{ memory: memLow, stale: true }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Breakdown" tags={['data-driven']} args={{ memory: memCrit }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="EmptyField" tags={['data-driven']} args={{ memory: memEmptyField }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story name="UnknownField" tags={['data-driven']} args={{ memory: memUnknownField }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="BreakdownLegend"
	tags={['interactive', 'data-driven']}
	args={{ memory: memLow }}
	play={async ({ canvasElement }) => {
		const legendItems = canvasElement.querySelectorAll('.lp-legend-item');
		await expect(legendItems.length, 'legend must list all 7 breakdown fields (no slicing)').toBe(
			7
		);

		for (const item of legendItems) {
			await expect(
				item.querySelector('.lp-legend-swatch'),
				'legend item must have a color swatch'
			).toBeTruthy();

			const fieldEl = item.querySelector('.lp-legend-field');
			await expect(fieldEl, 'legend item must have a field name').toBeTruthy();
			await expect(
				(fieldEl?.textContent ?? '').trim().length,
				'legend field name must not be empty'
			).toBeGreaterThan(0);

			const bytesEl = item.querySelector('.lp-legend-bytes');
			await expect(bytesEl, 'legend item must have a byte count').toBeTruthy();
			const bytesText = (bytesEl?.textContent ?? '').trim();
			await expect(
				bytesText.length,
				`legend bytes cell must not be empty, got "${bytesText}"`
			).toBeGreaterThan(0);
			await expect(
				bytesText.includes(' ') || bytesText.includes('B'),
				`legend bytes must include a unit suffix, got "${bytesText}"`
			).toBe(true);

			const shareEl = item.querySelector('.lp-legend-share');
			await expect(shareEl, 'legend item must have a share %').toBeTruthy();
			const shareText = (shareEl?.textContent ?? '').trim();
			await expect(
				shareText.includes('%'),
				`legend share must include "%", got "${shareText}"`
			).toBe(true);
		}

		const meter = canvasElement.querySelector('[role="meter"]');
		await expect(meter, 'Store gauge bar must have role=meter').not.toBeNull();

		const valuenow = Number(meter?.getAttribute('aria-valuenow'));
		await expect(valuenow, `aria-valuenow must be ≥ 0, got ${valuenow}`).toBeGreaterThanOrEqual(0);
		await expect(valuenow, `aria-valuenow must be ≤ 100, got ${valuenow}`).toBeLessThanOrEqual(100);

		await expect(meter?.getAttribute('aria-valuemin'), 'aria-valuemin must be "0"').toBe('0');
		await expect(meter?.getAttribute('aria-valuemax'), 'aria-valuemax must be "100"').toBe('100');
	}}
>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="MeterAriaOverBudget"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const meter = canvasElement.querySelector('[role="meter"]');
		await expect(meter, 'meter must be present').toBeTruthy();
		const valuenow = Number(meter?.getAttribute('aria-valuenow'));
		await expect(valuenow, 'aria-valuenow must be clamped at 100 when store exceeds budget').toBe(
			100
		);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memOver} />
		</div>
	{/snippet}
</Story>

<Story
	name="UnbudgetedNoBar"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const meter = canvasElement.querySelector('[role="meter"]');
		await expect(meter, 'no meter bar must render when max_store_bytes is 0').toBeNull();

		const text = canvasElement.textContent ?? '';
		await expect(
			text.includes('unbudgeted'),
			'"unbudgeted" text must appear when max_store_bytes is 0'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memUnbudgeted} />
		</div>
	{/snippet}
</Story>

<Story
	name="NoHistoryNoSparkline"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const sparklineRow = canvasElement.querySelector('.lp-sparkline-row');
		await expect(
			sparklineRow,
			'sparkline row must not render when history.length === 0'
		).toBeNull();
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memNoHistory} />
		</div>
	{/snippet}
</Story>

<Story
	name="HasHistorySparkline"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const sparklineRow = canvasElement.querySelector('.lp-sparkline-row');
		await expect(sparklineRow, 'sparkline row must render when history.length > 0').toBeTruthy();

		const label = canvasElement.querySelector('.lp-sparkline-label');
		await expect(label, 'sparkline sample count label must be present').toBeTruthy();
		await expect(label?.textContent?.includes('7'), 'sparkline label must show 7 samples').toBe(
			true
		);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memLow} />
		</div>
	{/snippet}
</Story>

<Story
	name="StaleOpacity"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const breakdown = canvasElement.querySelector('.lp-breakdown');
		await expect(breakdown, 'breakdown block must exist').toBeTruthy();
		await expect(
			breakdown?.classList.contains('lp-breakdown--stale'),
			'breakdown must carry lp-breakdown--stale modifier when stale=true'
		).toBe(true);

		const gaugeFill = canvasElement.querySelector('.lp-gauge-fill');
		await expect(gaugeFill, 'gauge fill must exist').toBeTruthy();
		await expect(
			gaugeFill?.classList.contains('lp-gauge-fill--stale'),
			'gauge fill must carry lp-gauge-fill--stale modifier when stale=true'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memLow} stale={true} />
		</div>
	{/snippet}
</Story>

<Story
	name="EmptyFieldLegendComplete"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const legendItems = canvasElement.querySelectorAll('.lp-legend-item');
		await expect(
			legendItems.length,
			'legend must list all 7 fields even when some have bytes===0'
		).toBe(7);

		const segments = canvasElement.querySelectorAll('.lp-share-seg');
		await expect(segments.length, 'bar must render only 3 segments for the 3 non-zero fields').toBe(
			3
		);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memEmptyField} />
		</div>
	{/snippet}
</Story>

<Story
	name="UnknownFieldFallback"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const legendItems = canvasElement.querySelectorAll('.lp-legend-item');
		await expect(legendItems.length, 'legend must include all fields including unknown ones').toBe(
			4
		);

		const segments = canvasElement.querySelectorAll('.lp-share-seg');
		await expect(segments.length, 'bar must render segments for all 4 non-zero fields').toBe(4);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memUnknownField} />
		</div>
	{/snippet}
</Story>

<Story
	name="BarSegmentTitles"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const segments = canvasElement.querySelectorAll('.lp-share-seg');
		await expect(
			segments.length,
			'must have at least one bar segment to assert titles on'
		).toBeGreaterThan(0);

		for (const seg of segments) {
			const title = seg.getAttribute('title') ?? '';
			await expect(
				title.includes('·'),
				`segment title must use "·" separators, got: "${title}"`
			).toBe(true);
			await expect(
				title.includes('%'),
				`segment title must include a percentage, got: "${title}"`
			).toBe(true);
		}
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memLow} />
		</div>
	{/snippet}
</Story>

<Story
	name="TooltipStore"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const labels = Array.from(canvasElement.querySelectorAll('.lp-gauge-label'));
		const storeLabel = labels.find((el) => el.textContent?.trim() === 'Store');
		await expect(storeLabel, '"Store" gauge label must be present').toBeTruthy();

		const rssLabel = labels.find((el) => el.textContent?.trim() === 'RSS');
		await expect(rssLabel, '"RSS" gauge label must be present').toBeTruthy();
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memLow} />
		</div>
	{/snippet}
</Story>

<Story
	name="RssNaText"
	tags={['interactive', 'data-driven']}
	args={{ memory: memRssNA }}
	play={async ({ canvasElement }) => {
		const gaugeValues = Array.from(canvasElement.querySelectorAll('.lp-gauge-value'));
		const rssValue = gaugeValues[1];
		await expect(rssValue, 'RSS gauge value element must be present').not.toBeNull();
		await expect(
			rssValue?.textContent?.trim(),
			'RSS gauge must show "n/a" when rss_bytes === 0'
		).toBe('n/a');
	}}
>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Snapshot"
	tags={['visual', 'data-driven']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ResourceGauges/LowUsage');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memLow} />
		</div>
	{/snippet}
</Story>

<Story
	name="WarnUsageSnapshot"
	tags={['visual', 'data-driven']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ResourceGauges/WarnUsage');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memWarn} />
		</div>
	{/snippet}
</Story>

<Story
	name="CritUsageSnapshot"
	tags={['visual', 'data-driven']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ResourceGauges/CritUsage');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memCrit} />
		</div>
	{/snippet}
</Story>

<Story
	name="OverBudgetSnapshot"
	tags={['visual', 'data-driven']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ResourceGauges/OverBudget');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memOver} />
		</div>
	{/snippet}
</Story>

<Story
	name="UnbudgetedSnapshot"
	tags={['visual', 'data-driven']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ResourceGauges/Unbudgeted');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memUnbudgeted} />
		</div>
	{/snippet}
</Story>

<Story
	name="StaleSnapshot"
	tags={['visual', 'data-driven']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ResourceGauges/Stale');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memLow} stale={true} />
		</div>
	{/snippet}
</Story>

<Story
	name="EmptyFieldSnapshot"
	tags={['visual', 'data-driven']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ResourceGauges/EmptyField');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memEmptyField} />
		</div>
	{/snippet}
</Story>

<Story
	name="UnknownFieldSnapshot"
	tags={['visual', 'data-driven']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ResourceGauges/UnknownField');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memUnknownField} />
		</div>
	{/snippet}
</Story>

<Story
	name="RssNASnapshot"
	tags={['visual', 'data-driven']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'ResourceGauges/RssNA');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22); max-width: 720px;">
			<ResourceGauges memory={memRssNA} />
		</div>
	{/snippet}
</Story>
