<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent } from 'storybook/test';
	import * as Slider from './index';

	const { Story } = defineMeta({
		title: 'UI/Slider',
		component: Slider.Root,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const thumb = canvas.getByRole('slider');
		await expect(thumb).toHaveAttribute('aria-valuenow', '50');

		thumb.focus();
		await userEvent.keyboard('{ArrowRight}');
		await expect(thumb).toHaveAttribute('aria-valuenow', '55');
	}}
>
	{#snippet template()}
		<div class="w-64 bg-[var(--surface-bg)] p-4">
			<Slider.Root value={50} min={0} max={100} step={5} aria-label="Example value" />
		</div>
	{/snippet}
</Story>
