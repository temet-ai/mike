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
	let key = $state('');
	let refusal = $state<string | null>(null);
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

		const below = canvas.getByTestId('below-key');
		const before = below.getBoundingClientRect().top;
		const keyInput = canvas.getByLabelText('Key column');
		await expect(keyInput).not.toHaveAttribute('aria-invalid');
		await userEvent.click(canvas.getByRole('button', { name: 'Refuse key' }));
		await expect(canvas.getByRole('alert')).toHaveTextContent('no such column: idd');
		await expect(keyInput).toHaveAttribute('aria-invalid', 'true');
		await expect(keyInput).toHaveAccessibleDescription('no such column: idd');
		await expect(below.getBoundingClientRect().top).toBe(before);
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
			<Field.Root label="Key column" bind:value={key} error={refusal} />
			<p data-testid="below-key" class="text-xs">Below the key column</p>
			<button type="button" onclick={() => (refusal = 'no such column: idd')}>Refuse key</button>
		</div>
	{/snippet}
</Story>
