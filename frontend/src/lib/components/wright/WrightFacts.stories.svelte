<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import WrightFacts from './WrightFacts.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightFacts',
		component: WrightFacts,
		tags: ['autodocs']
	});

	const LONG = `{"timestamp":"2026-11-14T19:27:02Z","level":"WARN","fields":{"message":"slow statement: execution time exceeded alert threshold"}}`;
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('failures')).toBeVisible();
		await expect(canvas.getByText('3')).toBeVisible();
		await expect(canvas.getByText(LONG)).toBeVisible();
		await expect(canvas.getByText(/"mean_ms": 1448/)).toBeVisible();
	}}
>
	{#snippet template()}
		<div style="padding: 16px; max-inline-size: 36rem; background: var(--surface-card);">
			<WrightFacts
				label="Latest evidence"
				facts={{
					failures: 3,
					window: '168 hours',
					sample_line: LONG,
					thresholds: { mean_ms: 1448, calls: 9 }
				}}
			/>
		</div>
	{/snippet}
</Story>
