<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import PromptKindCounts from './PromptKindCounts.svelte';

	const { Story } = defineMeta({
		title: 'Shared/PromptKindCounts',
		component: PromptKindCounts,
		tags: ['autodocs']
	});
</script>

<Story
	name="MixedDistribution"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const summary = canvas.getByTestId('prompt-kind-counts');
		await expect(summary).not.toBeNull();

		await expect(summary.textContent).toContain('23');
		await expect(summary.textContent).toContain('operator');
		await expect(summary.textContent).toContain('activity-label');
		await expect(summary.textContent).toContain('hook-feedback');
		await expect(summary.textContent).not.toContain('teammate-message');

		const segments = canvasElement.querySelectorAll('.pkc__seg');
		await expect(segments.length).toBe(4);
		await expect(summary.textContent).toContain('loop-wakeup');
		const operatorSeg = Array.from(segments).find((s) => s.textContent?.includes('operator'));
		await expect(operatorSeg).not.toBeUndefined();
		await expect(operatorSeg?.classList.contains('pkc__seg--operator')).toBe(true);
	}}
>
	{#snippet template()}
		<div style="padding:24px; background:var(--surface-bg);">
			<PromptKindCounts
				counts={{
					operator: 23,
					'activity-label': 2,
					'hook-feedback': 1,
					'teammate-message': 0,
					'loop-wakeup': 7
				}}
			/>
		</div>
	{/snippet}
</Story>
