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
	let mode = $state('wss');
	let refusal = $state<string | null>(null);
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

		const below = canvas.getByTestId('below-mode');
		const before = below.getBoundingClientRect().top;
		const mode = canvas.getByRole('combobox', { name: 'Mode' });
		await expect(mode).not.toHaveAttribute('aria-invalid');
		await userEvent.click(canvas.getByRole('button', { name: 'Refuse mode' }));
		await expect(canvas.getByRole('alert')).toHaveTextContent('mode is not supported');
		await expect(mode).toHaveAttribute('aria-invalid', 'true');
		await expect(below.getBoundingClientRect().top).toBe(before);
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
			<Select
				label="Mode"
				bind:value={mode}
				error={refusal}
				options={[{ value: 'wss' }, { value: 'https' }]}
			/>
			<p data-testid="below-mode" class="text-xs">Below the mode</p>
			<button type="button" onclick={() => (refusal = 'mode is not supported')}>Refuse mode</button>
		</div>
	{/snippet}
</Story>
