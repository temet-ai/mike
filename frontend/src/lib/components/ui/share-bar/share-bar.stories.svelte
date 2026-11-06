<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import ShareBar from './share-bar.svelte';

	const { Story } = defineMeta({
		title: 'UI/ShareBar',
		component: ShareBar,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'A proportional bar. One segment draws a row bar scaled against `max`; two segments draw a split bar scaled against their sum. The bar is hidden from assistive technology, so every value it shows is also printed as text beside it.'
				}
			}
		}
	});

	const rows = [
		{ label: 'Live', tone: 'live', value: 108 },
		{ label: 'Info', tone: 'info', value: 62 },
		{ label: 'Active', tone: 'active', value: 31 },
		{ label: 'Warn', tone: 'warn', value: 9 }
	] as const;
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const bars = canvasElement.querySelectorAll('[data-slot="share-bar"]');
		await expect(bars.length).toBe(5);
		for (const bar of bars) await expect(bar).toHaveAttribute('aria-hidden', 'true');

		const widths = (bar: Element) =>
			Array.from(bar.querySelectorAll<HTMLElement>('[data-slot="share-bar-segment"]')).map((seg) =>
				Math.round((seg.getBoundingClientRect().width / bar.getBoundingClientRect().width) * 100)
			);
		await expect(widths(bars[0]!)).toEqual([48, 52]);
		await expect(widths(bars[1]!)).toEqual([100]);
		await expect(widths(bars[2]!)).toEqual([57]);
		await expect(widths(bars[3]!)).toEqual([29]);
		await expect(widths(bars[4]!)).toEqual([8]);
		await expect(canvas.getByText('108 of 108')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="sbar-demo">
			<div class="sbar-demo__row">
				<span>Split</span>
				<ShareBar
					size="split"
					segments={[
						{ value: 108, tone: 'live' },
						{ value: 116, tone: 'info' }
					]}
				/>
				<span class="sbar-demo__num">108 and 116</span>
			</div>
			{#each rows as row (row.label)}
				<div class="sbar-demo__row">
					<span>{row.label}</span>
					<ShareBar segments={[{ value: row.value, tone: row.tone }]} max={108} />
					<span class="sbar-demo__num">{row.value} of 108</span>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<style>
	.sbar-demo {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		max-width: 32rem;
	}
	.sbar-demo__row {
		display: grid;
		grid-template-columns: 4rem minmax(0, 1fr) 6rem;
		align-items: center;
		gap: var(--space-3);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.sbar-demo__num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
</style>
