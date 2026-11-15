<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import KnowledgeKindTuneDialog from './KnowledgeKindTuneDialog.svelte';
	import { knowledgeKindsFixture } from '../../__fixtures__/knowledgeKinds';

	const { Story } = defineMeta({
		title: 'Components/Settings/KnowledgeKindTuneDialog',
		component: KnowledgeKindTuneDialog,
		tags: ['autodocs']
	});

	const invoice = knowledgeKindsFixture().find((k) => k.kind === 'invoice')!;
	let saved = $state('');
	let sent: unknown = null;
	let refuse = true;
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			saved = '';
			sent = null;
			refuse = true;
			globalThis.fetch = (async (_url: RequestInfo | URL, init?: RequestInit) => {
				sent = JSON.parse(String(init?.body));
				const json = { 'content-type': 'application/json' };
				if (refuse) {
					return new Response(
						JSON.stringify({ error: 'settle_secs must be at least 1', field: 'settle_secs' }),
						{ status: 400, headers: json }
					);
				}
				return new Response(JSON.stringify({ ...invoice, settle_secs: 90 }), {
					status: 200,
					headers: json
				});
			}) as typeof globalThis.fetch;
			return {};
		}
	]}
	play={async () => {
		const dialog = await within(document.body).findByRole('dialog');
		const body = within(dialog);
		await expect(dialog).toHaveTextContent('Tune Invoices');
		await expect(dialog).toHaveTextContent('finance.invoices');
		const save = body.getByRole('button', { name: 'Save' });
		await expect(save).toBeDisabled();

		// A refusal shows beside the field it names; the dialog stays open.
		const settle = body.getByLabelText(/Settle time/);
		await userEvent.clear(settle);
		await userEvent.type(settle, '0');
		await expect(save).toBeEnabled();
		await userEvent.click(save);
		await waitFor(() =>
			expect(settle).toHaveAccessibleDescription('settle_secs must be at least 1')
		);
		await expect(sent).toEqual({ settle_secs: 0 });
		await expect(saved).toBe('');

		refuse = false;
		await userEvent.clear(settle);
		await userEvent.type(settle, '90');
		await userEvent.click(save);
		await waitFor(() => expect(saved).toBe('invoice:90'));
		await expect(sent).toEqual({ settle_secs: 90 });
	}}
>
	{#snippet template()}
		<KnowledgeKindTuneDialog
			kind={invoice}
			onClose={() => undefined}
			onSaved={(row) => (saved = `${row.kind}:${row.settle_secs}`)}
		/>
	{/snippet}
</Story>
