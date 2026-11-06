<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import { Checkbox } from './index';

	const { Story } = defineMeta({
		title: 'UI/Checkbox',
		component: Checkbox,
		tags: ['autodocs']
	});

	let persist = $state(true);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const persistBox = canvas.getByRole('checkbox', { name: 'persist session' });
		await expect(persistBox).toBeChecked();
		await userEvent.click(persistBox);
		await expect(persistBox).not.toBeChecked();
		await expect(canvas.getByTestId('persist')).toHaveTextContent('false');
		persistBox.focus();
		await userEvent.keyboard(' ');
		await expect(persistBox).toBeChecked();
		await expect(canvas.getByRole('checkbox', { name: 'locked' })).toBeDisabled();
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-2 bg-[var(--surface-bg)] p-6">
			<Checkbox bind:checked={persist} label="persist session" />
			<Checkbox checked={false} label="raw frames" />
			<Checkbox checked={false} disabled label="locked" />
			<p data-testid="persist" class="text-xs">{persist}</p>
		</div>
	{/snippet}
</Story>
