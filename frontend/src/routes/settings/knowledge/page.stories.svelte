<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import KnowledgePage from './+page.svelte';
	import type { KnowledgePageData } from './+page';
	import type { KnowledgeKind } from '$lib/api/knowledgeKinds';
	import {
		KNOWLEDGE_PREVIEW_ROW,
		KNOWLEDGE_TABLES,
		knowledgeKind,
		knowledgeKindsFixture
	} from '$lib/components/__fixtures__/knowledgeKinds';

	let server: KnowledgeKind[] = [];

	function json(body: unknown, status = 200): Response {
		return new Response(JSON.stringify(body), {
			status,
			headers: { 'content-type': 'application/json' }
		});
	}

	function fetchStub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
		const url = String(input instanceof Request ? input.url : input);
		const method = init?.method ?? 'GET';
		const body = init?.body ? (JSON.parse(String(init.body)) as Record<string, unknown>) : {};
		if (url.endsWith('/kinds/tables')) return Promise.resolve(json({ tables: KNOWLEDGE_TABLES }));
		if (url.endsWith('/kinds/preview'))
			return Promise.resolve(json({ preview: KNOWLEDGE_PREVIEW_ROW }));
		if (url.endsWith('/kinds') && method === 'GET') return Promise.resolve(json({ kinds: server }));
		if (url.endsWith('/kinds') && method === 'POST') {
			server = [
				...server,
				knowledgeKind({
					kind: String(body.kind),
					label: String(body.label),
					built_in: false,
					definition: {
						source_table: String(body.source_table),
						key_column: String(body.key_column),
						title_column: String(body.title_column),
						body_columns: body.body_columns as string[],
						project_column: null
					}
				})
			];
			return Promise.resolve(json({ kind: String(body.kind) }, 201));
		}
		const kind = decodeURIComponent(url.split('/kinds/').pop() ?? '');
		if (method === 'PATCH') {
			server = server.map((k) => (k.kind === kind ? { ...k, ...body } : k));
			return Promise.resolve(json(server.find((k) => k.kind === kind)));
		}
		return Promise.resolve(json({ error: 'unexpected fetch' }, 503));
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Knowledge',
		component: KnowledgePage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});

	function data(): KnowledgePageData {
		return { kinds: knowledgeKindsFixture(), loadError: null };
	}

	function order(canvasElement: HTMLElement): (string | null | undefined)[] {
		return [
			...canvasElement.querySelectorAll('tbody [data-slot="data-table-row"] .kk-kind .kk-sub')
		].map((e) => e.textContent);
	}
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			server = knowledgeKindsFixture();
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: data() }}
	play={async ({ canvas, canvasElement }) => {
		await expect(canvas.getByRole('link', { name: 'Knowledge' })).toHaveAttribute(
			'aria-current',
			'page'
		);
		await expect(order(canvasElement)).toEqual(['artifact', 'invoice', 'lesson', 'turn']);

		// A switch saves and keeps its place.
		await userEvent.click(canvas.getByRole('switch', { name: 'Embed Lessons' }));
		await waitFor(() => expect(canvas.getByText('Saved lesson')).toBeVisible());
		await expect(server.find((k) => k.kind === 'lesson')?.enabled).toBe(true);

		// Add a kind end to end: it lands in sorted place.
		await userEvent.type(canvas.getByLabelText(/^Name/), 'memo');
		await userEvent.type(canvas.getByLabelText(/^Label/), 'Memos');
		await userEvent.type(canvas.getByLabelText(/^Table/), 'finance.credits');
		await userEvent.type(canvas.getByLabelText(/^Key column/), 'id');
		await userEvent.type(canvas.getByLabelText(/^Title column/), 'memo');
		await userEvent.type(canvas.getByLabelText(/^Body columns/), 'memo');
		await userEvent.click(canvas.getByRole('button', { name: 'Preview' }));
		const add = canvas.getByRole('button', { name: 'Add source' });
		await waitFor(() => expect(add).toBeEnabled());
		await userEvent.click(add);
		await waitFor(() => expect(canvas.getByText('Added memo')).toBeVisible());
		await waitFor(() =>
			expect(order(canvasElement)).toEqual(['artifact', 'invoice', 'lesson', 'memo', 'turn'])
		);

		// A poll with new counts leaves the order alone.
		server = server.map((k) =>
			k.kind === 'turn' ? { ...k, stats: { ...k.stats, items: 99999, embedded: 99999 } } : k
		);
		await waitFor(
			() => expect(within(canvasElement).getAllByText('99,999').length).toBeGreaterThan(0),
			{
				timeout: 6000
			}
		);
		await expect(order(canvasElement)).toEqual(['artifact', 'invoice', 'lesson', 'memo', 'turn']);
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<KnowledgePage {...args} />
		</div>
	{/snippet}
</Story>
