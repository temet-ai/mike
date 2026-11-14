<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import WrightChangeCard from './WrightChangeCard.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightChangeCard',
		component: WrightChangeCard,
		tags: ['autodocs']
	});

	const onUndo = fn();
</script>

<Story
	name="Default"
	play={async ({ canvas }) => {
		await expect(canvas.getByText('Paused: Close orphan launch')).toBeVisible();
		await expect(canvas.getByText('Until the fleet closes')).toBeVisible();
		await userEvent.click(
			canvas.getByRole('button', { name: 'Undo: Paused: Close orphan launch' })
		);
		await expect(onUndo).toHaveBeenCalledOnce();
		await expect(canvas.getByText('Limit changed: Daily starts')).toBeVisible();
		await expect(canvas.getByRole('alert')).toHaveTextContent("Couldn't undo: Changed since.");
		await expect(canvas.getByText('Undone')).toBeVisible();
	}}
>
	{#snippet template()}
		<div
			style="display: flex; flex-direction: column; gap: 8px; padding: 16px; max-inline-size: 40rem; background: var(--surface-card);"
		>
			<WrightChangeCard
				change={{ type: 'pause_remedy', name: 'close_orphan_launch', until: 'the fleet closes' }}
				undo="ready"
				{onUndo}
			/>
			<WrightChangeCard
				change={{ type: 'set_engineer_limit', field: 'daily_starts', value: 2 }}
				undo="failed"
				undoReason="Changed since."
				{onUndo}
			/>
			<WrightChangeCard
				change={{ type: 'open_problem', check: 'log_errors', resource: 'mike log' }}
				undo="undone"
				{onUndo}
			/>
		</div>
	{/snippet}
</Story>
