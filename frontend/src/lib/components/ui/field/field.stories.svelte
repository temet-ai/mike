<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import * as Field from './index';

	const { Story } = defineMeta({
		title: 'UI/Field',
		component: Field.Root,
		tags: ['autodocs']
	});

	let name = $state('');
	let goal = $state('');
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const input = canvas.getByLabelText('Name');
		await userEvent.type(input, '/mnt/data/projects/a-very-long-path-that-does-not-fit');
		await expect(input).toHaveValue('/mnt/data/projects/a-very-long-path-that-does-not-fit');
		await userEvent.type(canvas.getByLabelText(/Goal/), 'ship it');
		await expect(canvas.getByLabelText(/Goal/)).toHaveValue('ship it');
		await expect(canvas.getByLabelText('Locked')).toBeDisabled();
	}}
>
	{#snippet template()}
		<div class="flex w-80 flex-col gap-4 bg-[var(--surface-bg)] p-4">
			<Field.Root label="Name" bind:value={name} placeholder="Name shown in the list" />
			<Field.Root
				label="Goal"
				hint="(optional)"
				multiline
				rows={2}
				bind:value={goal}
				placeholder="What this is trying to do"
			/>
			<Field.Root label="Locked" value="read only" disabled />
			<Field.Root type="search" aria-label="Find" placeholder="Find" />
		</div>
	{/snippet}
</Story>
