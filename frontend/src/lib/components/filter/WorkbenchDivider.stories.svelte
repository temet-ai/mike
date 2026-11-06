<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import WorkbenchDivider from './WorkbenchDivider.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchDivider',
		component: WorkbenchDivider,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let width = $state(230);
	let deltas: number[] = $state([]);

	function onResize(deltaPx: number): void {
		width = Math.max(180, Math.min(420, width + deltaPx));
		deltas = [...deltas, deltaPx];
	}
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const handle = await canvas.findByRole('separator', { name: 'Resize outline' });

		await expect(handle).toHaveAttribute('aria-orientation', 'vertical');
		await expect(handle).toHaveAttribute('aria-valuenow', '230');
		await expect(handle).toHaveAttribute('aria-valuemin', '180');
		await expect(handle).toHaveAttribute('aria-valuemax', '420');

		handle.focus();
		await userEvent.keyboard('{ArrowRight}');
		await waitFor(() => expect(handle).toHaveAttribute('aria-valuenow', '246'));

		await userEvent.keyboard('{ArrowLeft}');
		await waitFor(() => expect(handle).toHaveAttribute('aria-valuenow', '230'));
	}}
>
	{#snippet template()}
		<div
			style="display: flex; height: 120px; width: {width}px; max-width: 420px; background: var(--surface-bg); border: 1px solid var(--border-default);"
		>
			<div style="flex: 1; padding: 8px; color: var(--text-2); font-size: 11px;">
				{width}px — drag or focus + arrow keys
			</div>
			<WorkbenchDivider
				orientation="vertical"
				label="Resize outline"
				value={width}
				min={180}
				max={420}
				{onResize}
			/>
		</div>
	{/snippet}
</Story>
