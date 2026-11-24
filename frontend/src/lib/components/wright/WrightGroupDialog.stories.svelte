<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, screen, userEvent, waitFor, within } from 'storybook/test';
	import type { GroupTarget } from '$lib/wright/store.svelte';
	import { openDialog } from '../__fixtures__/dialogs';
	import WrightGroupDialog from './WrightGroupDialog.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightGroupDialog',
		component: WrightGroupDialog,
		tags: ['autodocs']
	});

	const GROUPS = [
		{ id: 3, name: 'Mail delivery', causes: 2 },
		{ id: 4, name: 'Tools', causes: 1 }
	];

	let open = $state(true);
	const onConfirm = fn(async (_target: GroupTarget | null) => true);
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
		const dialog = within(await openDialog('Group 2 causes'));
		const choice = dialog.getByRole('combobox', { name: 'Group' });

		// A name that is taken is refused where it is typed, and the button stays off.
		await userEvent.selectOptions(choice, 'A new group');
		const name = dialog.getByRole('textbox', { name: 'Name' });
		await userEvent.type(name, 'Mail delivery');
		await expect(dialog.getByText(/already exists/)).toBeVisible();
		await expect(dialog.getByRole('button', { name: 'Group' })).toBeDisabled();

		await userEvent.clear(name);
		await userEvent.type(name, '  Hooks ');
		await expect(dialog.getByRole('button', { name: 'Group' })).toBeEnabled();
		await userEvent.click(dialog.getByRole('button', { name: 'Group' }));
		await expect(onConfirm).toHaveBeenLastCalledWith({ name: 'Hooks' });
		await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
	}}
>
	{#snippet template()}
		<WrightGroupDialog bind:open count={2} groups={GROUPS} {onConfirm} />
	{/snippet}
</Story>
