<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import KnowledgeKindForm from './KnowledgeKindForm.svelte';
	import { KNOWLEDGE_PREVIEW_ROW, KNOWLEDGE_TABLES } from '../../__fixtures__/knowledgeKinds';

	const { Story } = defineMeta({
		title: 'Components/Settings/KnowledgeKindForm',
		component: KnowledgeKindForm,
		tags: ['autodocs']
	});

	let added = $state('');
	let created: Record<string, unknown> | null = null;

	function json(body: unknown, status = 200): Response {
		return new Response(JSON.stringify(body), {
			status,
			headers: { 'content-type': 'application/json' }
		});
	}

	function fetchStub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
		const url = String(input instanceof Request ? input.url : input);
		if (url.endsWith('/tables')) return Promise.resolve(json({ tables: KNOWLEDGE_TABLES }));
		const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
		if (url.endsWith('/preview')) {
			if (body.key_column === 'idd') {
				return Promise.resolve(
					json(
						{ error: 'finance.invoices has no column idd', field: 'key_column', watch_index: null },
						400
					)
				);
			}
			return Promise.resolve(json({ preview: KNOWLEDGE_PREVIEW_ROW }));
		}
		created = body;
		return Promise.resolve(json({ kind: String(body.kind) }, 201));
	}
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			added = '';
			created = null;
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	play={async ({ canvas, canvasElement }) => {
		const add = canvas.getByRole('button', { name: 'Add source' });
		await expect(add).toBeDisabled();
		await expect(canvas.getByText(/Press Preview/)).toBeVisible();

		await userEvent.type(canvas.getByLabelText(/^Name/), 'invoice2');
		await userEvent.type(canvas.getByLabelText(/^Label/), 'Invoices two');
		await userEvent.type(canvas.getByLabelText(/^Table/), 'finance.invoices');
		const key = canvas.getByLabelText(/^Key column/);
		await userEvent.type(key, 'idd');
		await userEvent.type(canvas.getByLabelText(/^Title column/), 'supplier');
		await userEvent.type(canvas.getByLabelText(/^Body columns/), 'notes, lines');
		await waitFor(() => expect(canvas.getAllByText(/Columns: id, supplier/)).toHaveLength(2));

		// A refusal naming key_column shows under Key column and moves nothing.
		const settle = canvas.getByLabelText(/^Settle time/);
		const name = canvas.getByLabelText(/^Name/);
		const gap = () => settle.getBoundingClientRect().top - name.getBoundingClientRect().top;
		const before = gap();
		await userEvent.click(canvas.getByRole('button', { name: 'Preview' }));
		await waitFor(() =>
			expect(key).toHaveAccessibleDescription('finance.invoices has no column idd')
		);
		await expect(key).toHaveAttribute('aria-invalid', 'true');
		await expect(gap()).toBe(before);
		await expect(add).toBeDisabled();

		// A good preview enables Add; editing the table makes it stale again.
		await userEvent.clear(key);
		await userEvent.type(key, 'id');
		await userEvent.click(canvas.getByRole('button', { name: 'Preview' }));
		await waitFor(() =>
			expect(
				canvas.getByText('Delivered late, credit note agreed.', { exact: false })
			).toBeVisible()
		);
		await expect(add).toBeEnabled();
		const table = canvas.getByLabelText(/^Table/);
		await userEvent.type(table, '2');
		await expect(add).toBeDisabled();
		await expect(canvas.getByText(/Press Preview/)).toBeVisible();
		await userEvent.type(table, '{Backspace}');
		await userEvent.click(canvas.getByRole('button', { name: 'Preview' }));
		await waitFor(() => expect(add).toBeEnabled());

		// A view shows its own block and keeps what was typed.
		await userEvent.click(canvas.getByRole('radio', { name: 'A view' }));
		await expect(canvas.getByLabelText(/^View name/)).toBeVisible();
		await expect(canvas.queryByLabelText(/^Title column/)).toBeNull();
		await userEvent.click(canvas.getByRole('radio', { name: 'A table' }));
		await expect(canvas.queryByLabelText(/^View name/)).toBeNull();
		await expect(canvas.getByLabelText(/^Title column/)).toHaveValue('supplier');
		await userEvent.click(canvas.getByRole('button', { name: 'Preview' }));
		await waitFor(() => expect(add).toBeEnabled());

		await userEvent.click(add);
		await waitFor(() => expect(added).toBe('invoice2'));
		await expect(created).toMatchObject({
			kind: 'invoice2',
			source_table: 'finance.invoices',
			key_column: 'id',
			title_column: 'supplier',
			body_columns: ['notes', 'lines'],
			view: null
		});
		await expect(canvas.getByLabelText(/^Name/)).toHaveValue('');
		await expect(canvasElement.querySelectorAll('[role="alert"]').length).toBeGreaterThan(0);
	}}
>
	{#snippet template()}
		<div class="w-[900px] bg-[var(--surface-card)] p-4">
			<KnowledgeKindForm onAdded={(kind) => (added = kind)} />
		</div>
	{/snippet}
</Story>
