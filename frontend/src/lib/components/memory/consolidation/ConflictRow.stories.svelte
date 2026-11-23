<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import ConflictRow from './ConflictRow.svelte';
	import { CONFLICTS } from '$lib/test-support/__fixtures__/knowledgeConsolidation';

	const { Story } = defineMeta({
		title: 'Components/Memory/ConflictRow',
		component: ConflictRow,
		tags: ['autodocs']
	});
	const onKeep = fn();
</script>

<Story
	name="Open"
	play={async ({ canvas }) => {
		await expect(canvas.getAllByText('Conflict').length).toBe(2);
		await expect(canvas.getByText(/Both lessons were stated within a day/)).toBeInTheDocument();
		await expect(canvas.getByText(/Both files were written within a day/)).toBeInTheDocument();
		await userEvent.click(canvas.getAllByRole('button', { name: 'Keep this one' })[1]!);
		await expect(onKeep).toHaveBeenCalledWith(8102);
		await expect(canvas.getByText('Settled')).toBeInTheDocument();
		await expect(
			canvas.getByText('Not kept. The decision was already settled.')
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="edge mech-group w-[960px]">
			<ConflictRow row={CONFLICTS[0]!} project="3" {onKeep} />
			<ConflictRow row={CONFLICTS[1]!} project="3" {onKeep} />
			<ConflictRow
				row={CONFLICTS[0]!}
				project="3"
				settled="Lesson 2733"
				error="Not kept. The decision was already settled."
				{onKeep}
			/>
		</div>
	{/snippet}
</Story>
