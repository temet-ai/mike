<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import NewRowsButton from './NewRowsButton.svelte';

	const { Story } = defineMeta({
		title: 'Components/Memory/NewRowsButton',
		component: NewRowsButton,
		tags: ['autodocs']
	});
	const onShow = fn();
</script>

<Story
	name="Slot"
	play={async ({ canvasElement, canvas }) => {
		const slots = canvasElement.querySelectorAll<HTMLElement>('[data-slot="new-rows-button"]');
		await expect(slots[0]!.getBoundingClientRect().width).toBeCloseTo(
			slots[1]!.getBoundingClientRect().width,
			1
		);
		await expect(getComputedStyle(slots[0]!).visibility).toBe('hidden');
		await userEvent.click(canvas.getByRole('button', { name: 'Show 3 new' }));
		await expect(onShow).toHaveBeenCalledTimes(1);
	}}
>
	{#snippet template()}
		<div class="flex w-80 flex-col gap-2">
			<NewRowsButton count={0} {onShow} />
			<NewRowsButton count={3} {onShow} />
		</div>
	{/snippet}
</Story>
