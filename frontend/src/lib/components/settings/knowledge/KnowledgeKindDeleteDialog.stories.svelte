<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import KnowledgeKindDeleteDialog from './KnowledgeKindDeleteDialog.svelte';
	import { knowledgeKindsFixture } from '../../__fixtures__/knowledgeKinds';

	const { Story } = defineMeta({
		title: 'Components/Settings/KnowledgeKindDeleteDialog',
		component: KnowledgeKindDeleteDialog,
		tags: ['autodocs']
	});

	const invoice = knowledgeKindsFixture().find((k) => k.kind === 'invoice')!;
	let deleted = $state('');
	let refuse = true;
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			deleted = '';
			refuse = true;
			globalThis.fetch = (async () =>
				refuse
					? new Response(JSON.stringify({ error: 'invoice is built in', field: null }), {
							status: 400,
							headers: { 'content-type': 'application/json' }
						})
					: new Response(null, { status: 204 })) as typeof globalThis.fetch;
			return {};
		}
	]}
	play={async () => {
		const dialog = await within(document.body).findByRole('dialog');
		await expect(dialog).toHaveTextContent('Delete Invoices?');
		await expect(dialog).toHaveTextContent('Removes its 31 items and 28 embeddings.');
		await expect(dialog).toHaveTextContent('finance.invoices');
		const body = within(dialog);
		await waitFor(() => expect(body.getByRole('button', { name: 'Cancel' })).toHaveFocus());

		// A refusal keeps the dialog open and names the reason.
		await userEvent.click(body.getByRole('button', { name: 'Delete' }));
		await expect(await body.findByRole('alert')).toHaveTextContent('invoice is built in');
		await expect(deleted).toBe('');

		refuse = false;
		await userEvent.click(body.getByRole('button', { name: 'Delete' }));
		await waitFor(() => expect(deleted).toBe('invoice'));
	}}
>
	{#snippet template()}
		<KnowledgeKindDeleteDialog
			kind={invoice}
			onClose={() => undefined}
			onDeleted={(kind) => (deleted = kind)}
		/>
	{/snippet}
</Story>
