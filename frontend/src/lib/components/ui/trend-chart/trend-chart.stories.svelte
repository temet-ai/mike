<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import TrendChart from './trend-chart.svelte';
	import { formatSignedCount, formatSignedPoints, type TrendPoint } from './trend';

	const { Story } = defineMeta({
		title: 'UI/TrendChart',
		component: TrendChart,
		tags: ['autodocs']
	});

	const START = Date.parse('2026-10-21T16:40:00Z');
	const DAY = 86_400_000;
	const times = Array.from({ length: 30 }, (_, i) => new Date(START + i * DAY).toISOString());
	const series = (value: (i: number) => number): TrendPoint[] =>
		times.map((at, i) => ({ at, value: value(i) }));

	const FALLING = series((i) => 2044 - Math.round((i * 132) / 29));
	const RISING = series((i) => 2 + Math.round((i * 5) / 29));
	const FLAT = series(() => 1604);
	const SHARE = series((i) => 0.048 - (i * 0.006) / 29);
	const domain: [number, number] = [START, START + 29 * DAY];
	const count = (n: number) => Math.round(n).toLocaleString('en-US');
	const percent = (n: number) => `${(n * 100).toFixed(1)}%`;
</script>

<script lang="ts">
	let pointAt = $state<number | null>(null);
	let group: HTMLElement | undefined = $state();
</script>

<Story
	name="Group"
	play={async ({ canvasElement }) => {
		const plots = canvasElement.querySelectorAll<HTMLElement>('.tc__plot');
		await expect(plots).toHaveLength(4);
		const box = plots[1]!.getBoundingClientRect();
		await userEvent.pointer({
			target: plots[1]!,
			coords: { clientX: box.left + box.width / 2, clientY: box.top + 20 }
		});
		await waitFor(() => expect(canvasElement.querySelectorAll('.tc__pointer')).toHaveLength(4));
		await expect(canvasElement.querySelectorAll('.tc__pointed-value')).toHaveLength(4);

		await userEvent.unhover(plots[1]!);
		await waitFor(() => expect(canvasElement.querySelectorAll('.tc__pointer')).toHaveLength(0));

		group!.focus();
		await userEvent.keyboard('{ArrowLeft}');
		await waitFor(() => expect(canvasElement.querySelectorAll('.tc__pointer')).toHaveLength(4));
	}}
>
	{#snippet template()}
		<!-- the group is one tab stop with arrow keys, as in MeasuresBand -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<div
			bind:this={group}
			class="grid w-[960px] grid-cols-4 gap-px bg-[var(--border-subtle)]"
			role="group"
			tabindex="0"
			aria-label="Charts"
			onkeydown={(e) => {
				const i = pointAt === null ? 29 : Math.round((pointAt - START) / DAY);
				if (e.key === 'ArrowLeft') pointAt = START + Math.max(0, i - 1) * DAY;
				if (e.key === 'ArrowRight') pointAt = START + Math.min(29, i + 1) * DAY;
			}}
		>
			{#each [{ label: 'Statements in force', points: FALLING, format: count, delta: formatSignedCount, better: 'lower' as const, tip: 'Single instructions or facts currently served.' }, { label: 'Open conflicts', points: RISING, format: count, delta: formatSignedCount, better: 'lower' as const, tip: undefined }, { label: 'Subjects', points: FLAT, format: count, delta: formatSignedCount, better: undefined, tip: undefined }, { label: 'Misgrouped', points: SHARE, format: percent, delta: formatSignedPoints, better: 'lower' as const, tip: undefined }] as chart (chart.label)}
				<div class="bg-[var(--surface-card)] p-3">
					<TrendChart
						label={chart.label}
						tooltip={chart.tip}
						points={chart.points}
						format={chart.format}
						formatDelta={chart.delta}
						better={chart.better}
						{domain}
						bind:pointAt
					/>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
