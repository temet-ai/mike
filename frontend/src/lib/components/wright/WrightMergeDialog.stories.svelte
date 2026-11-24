<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, screen, userEvent, waitFor, within } from 'storybook/test';
	import { causeFixture } from '../__fixtures__/wright';
	import { openDialog } from '../__fixtures__/dialogs';
	import WrightMergeDialog from './WrightMergeDialog.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightMergeDialog',
		component: WrightMergeDialog,
		tags: ['autodocs']
	});

	const CAUSES = [
		causeFixture({
			cause_id: 18,
			check_name: 'slow_queries',
			resource: 'query 1: SELECT * FROM memory.operator_turns',
			problems: 7
		}),
		causeFixture({
			cause_id: 19,
			check_name: 'idle_polls',
			resource: 'query 2: SELECT 1 FROM memory.operator_turns',
			problems: 2
		}),
		causeFixture({
			cause_id: 20,
			check_name: 'reads_outpace_writes',
			resource: 'query 3: SELECT * FROM memory.operator_turns WHERE session_id = $1',
			problems: 1
		})
	];

	let open = $state(true);
	const onConfirm = fn(async (_root: number, _ids: number[]) => true);
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
		const dialog = within(await openDialog('Merge 3 causes'));

		// The highest-ranked cause keeps the others to begin with.
		const group = dialog.getByRole('radiogroup', { name: 'Merge into' });
		await expect(within(group).getByRole('radio', { name: /slow_queries/ })).toBeChecked();
		await expect(dialog.getByText(/3 problems move into slow_queries\./)).toBeVisible();
		// What Separate can and cannot undo is said before the button.
		await expect(dialog.getByText(/Separate on the kept cause moves those problems back/)).toBeVisible();
		await expect(dialog.getByText(/cannot be separated/)).toBeVisible();

		// Picking another cause changes who keeps the rest and what moves.
		await userEvent.click(within(group).getByRole('radio', { name: /idle_polls/ }));
		await expect(dialog.getByText(/8 problems move into idle_polls\./)).toBeVisible();

		await userEvent.click(dialog.getByRole('button', { name: 'Merge' }));
		await expect(onConfirm).toHaveBeenCalledWith(19, [18, 20]);
		await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
	}}
>
	{#snippet template()}
		<WrightMergeDialog bind:open causes={CAUSES} {onConfirm} />
	{/snippet}
</Story>
