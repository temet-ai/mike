<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import { Select } from './index';

	const { Story } = defineMeta({
		title: 'UI/Select',
		component: Select,
		tags: ['autodocs']
	});

	let protocol = $state('wss');
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const select = canvas.getByRole('combobox', { name: 'Protocol' });
		await expect(select).toHaveValue('wss');
		await userEvent.selectOptions(select, 'https');
		await expect(select).toHaveValue('https');
		await expect(canvas.getByTestId('protocol')).toHaveTextContent('https');
		await expect(within(select).getByRole('option', { name: 'raw' })).toBeDisabled();
	}}
>
	{#snippet template()}
		<div class="w-72 bg-[var(--surface-bg)] p-6">
			<Select
				label="Protocol"
				bind:value={protocol}
				options={[{ value: 'wss' }, { value: 'https' }, { value: 'raw', disabled: true }]}
			/>
			<p data-testid="protocol" class="mt-2 text-xs">{protocol}</p>
		</div>
	{/snippet}
</Story>
