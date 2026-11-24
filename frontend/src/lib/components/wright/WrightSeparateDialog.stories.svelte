<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, screen, userEvent, waitFor, within } from 'storybook/test';
	import { absorbedFixture } from '../__fixtures__/wright';
	import { openDialog } from '../__fixtures__/dialogs';
	import WrightSeparateDialog from './WrightSeparateDialog.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightSeparateDialog',
		component: WrightSeparateDialog,
		tags: ['autodocs']
	});

	let open = $state(true);
	const onConfirm = fn(async (_id: number) => true);
</script>

<Story
	name="Default"
	loaders={[
		() => {
			open = true;
			return {};
		}
	]}
	play={async () => {
		const dialog = within(await openDialog('Separate this merge'));
		// What comes back, and what stays where it is.
		await expect(dialog.getByText(/with the 4 problems that moved when you merged it/)).toBeVisible();
		await expect(
			dialog.getByText(/Problems opened after the merge stay with idle_polls/)
		).toBeVisible();

		await userEvent.click(dialog.getByRole('button', { name: 'Cancel' }));
		await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
		open = true;
		await userEvent.click(
			within(await openDialog('Separate this merge')).getByRole('button', { name: 'Separate' })
		);
		await expect(onConfirm).toHaveBeenCalledWith(5);
		await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
	}}
>
	{#snippet template()}
		<WrightSeparateDialog bind:open merge={absorbedFixture()} keptBy="idle_polls" {onConfirm} />
	{/snippet}
</Story>
