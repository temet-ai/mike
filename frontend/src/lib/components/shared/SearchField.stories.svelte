<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import SearchField from './SearchField.svelte';

	const onClear = fn();

	const { Story } = defineMeta({
		title: 'Shared/SearchField',
		component: SearchField,
		tags: ['autodocs']
	});

	let query = $state('');
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const input = canvas.getByRole('searchbox', { name: 'Search sessions or content' });
		await expect(canvas.queryByRole('button', { name: 'Clear search' })).toBeNull();
		const before = input.getBoundingClientRect();
		await userEvent.type(input, 'auth');
		await expect(canvas.getByTestId('query')).toHaveTextContent('auth');
		const clear = await canvas.findByRole('button', { name: 'Clear search' });
		const after = input.getBoundingClientRect();
		await expect(after.width, 'typing must not resize the field').toBe(before.width);
		await userEvent.click(clear);
		await expect(onClear).toHaveBeenCalledOnce();
		await expect(input).toHaveValue('');
		await expect(input).toHaveFocus();
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-6">
			<SearchField
				bind:value={query}
				label="Search sessions or content"
				placeholder="Search sessions or content"
				{onClear}
			/>
			<p data-testid="query" class="mt-2 text-xs">{query}</p>
		</div>
	{/snippet}
</Story>
