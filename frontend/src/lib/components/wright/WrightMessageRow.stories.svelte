<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import { messages } from '../__fixtures__/wright';
	import WrightMessageRow from './WrightMessageRow.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightMessageRow',
		component: WrightMessageRow,
		tags: ['autodocs']
	});

	const onUndoSteer = fn();
	const ROWS = messages();
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getAllByText('You', { selector: '.msg__sender' })).toHaveLength(2);
		await expect(canvas.getByRole('link', { name: /Problem 703/ })).toHaveAttribute(
			'href',
			'#problem-703'
		);
		await expect(canvas.getByText('The mail itself stays queued.')).toBeVisible();
		await userEvent.click(canvas.getByRole('button', { name: /^Undo: Paused/ }));
		await expect(onUndoSteer).toHaveBeenCalledWith(ROWS[3]);
	}}
>
	{#snippet template()}
		<ol
			style="list-style: none; margin: 0; padding: 0 16px; max-inline-size: 44rem; background: var(--surface-card);"
		>
			{#each ROWS as message (message.id)}
				<WrightMessageRow {message} undo={{}} {onUndoSteer} />
			{/each}
		</ol>
	{/snippet}
</Story>
