<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import ChangeRow from './ChangeRow.svelte';
	import { CHANGES } from '$lib/test-support/__fixtures__/knowledgeConsolidation';

	const { Story } = defineMeta({
		title: 'Components/Memory/ChangeRow',
		component: ChangeRow,
		tags: ['autodocs']
	});
	const onAction = fn();
</script>

<Story
	name="Verdicts"
	play={async ({ canvas }) => {
		for (const word of ['Covered', 'Shortened', 'Superseded', 'Merged', 'Moved', 'Kept']) {
			await expect(canvas.getByText(word)).toBeInTheDocument();
		}
		for (const word of ['Applied', 'Proposed', 'Failed', 'Undone', 'No edit']) {
			await expect(canvas.getAllByText(word).length).toBeGreaterThan(0);
		}
		await expect(canvas.getByText('After')).toBeInTheDocument();
		await expect(canvas.getAllByText('Before').length).toBe(1);
		await expect(canvas.getAllByText('Retired').length).toBe(3);
		await expect(canvas.getByText(/Not written: permission denied/)).toBeInTheDocument();
		await userEvent.click(canvas.getByRole('button', { name: 'Write to file' }));
		await expect(onAction).toHaveBeenCalledWith('apply');
		await expect(canvas.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="edge mech-group w-[960px]">
			{#each CHANGES as row (row.id)}
				<ChangeRow {row} project="3" {onAction} />
			{/each}
		</div>
	{/snippet}
</Story>
