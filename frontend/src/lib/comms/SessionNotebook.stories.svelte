<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import SessionNotebook from './SessionNotebook.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';
	import type { SessionNoteDto } from './sessionNotes';
	import 'augmented-ui/augmented-ui.min.css';

	const SESSION_ID = '872d7da7-a414-4432-bef2-630c65ef66b5';

	function makeNote(partial: Partial<SessionNoteDto> & { note_uuid: string }): SessionNoteDto {
		return {
			kind: 'note',
			title: 'untitled',
			body: '',
			status: null,
			tags: [],
			data: {},
			created_at: '2026-09-19T10:00:00Z',
			updated_at: '2026-09-19T10:00:00Z',
			...partial
		};
	}

	let notes: SessionNoteDto[] = [
		makeNote({
			note_uuid: 'a1111111-0000-4000-8000-000000000001',
			kind: 'finding',
			title: 'Mailbox fan-out drops role: recipients when the roster is stale',
			body: 'Traced through resolve_role_recipients — a session whose lease expired within the last heartbeat window is still resolved as live, so the message is queued but never delivered.',
			tags: ['mailbox', 'blocker'],
			created_at: '2026-09-19T09:40:00Z',
			updated_at: '2026-09-19T09:40:00Z'
		}),
		makeNote({
			note_uuid: 'a1111111-0000-4000-8000-000000000002',
			kind: 'task',
			title: 'Wire the identity card into the fleet inspector',
			status: 'done',
			created_at: '2026-09-19T08:10:00Z',
			updated_at: '2026-09-19T09:00:00Z'
		})
	];

	function router(method: string, url: string, body: unknown): TypedBody {
		if (method === 'POST' && url.includes('/notes')) {
			const b = body as {
				kind?: string;
				title: string;
				body?: string;
				status?: string;
				tags?: string[];
			};
			const created = makeNote({
				note_uuid: `generated-${notes.length}`,
				kind: b.kind ?? 'note',
				title: b.title,
				body: b.body ?? '',
				status: b.status ?? null,
				tags: b.tags ?? [],
				created_at: '2026-09-19T10:05:00Z',
				updated_at: '2026-09-19T10:05:00Z'
			});
			notes = [created, ...notes];
			return typed<SessionNoteDto>(created);
		}
		if (url.includes('/notes')) return typed<SessionNoteDto[]>(notes);
		return typed<Record<string, never>>({});
	}

	let calls = 0;

	const { Story } = defineMeta({
		title: 'Comms/SessionNotebook',
		component: SessionNotebook,
		tags: ['autodocs']
	});
</script>

<Story
	name="WithFindingsAndTasks"
	args={{ sessionId: SESSION_ID }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const toggle = canvas.getByRole('button', { name: 'Notebook' });
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await expect(canvas.queryByText(/Mailbox fan-out drops/)).toBeNull();

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));
		await waitFor(() => expect(canvas.getByText(/Mailbox fan-out drops/)).toBeInTheDocument());
		await expect(canvas.getByText('finding')).toBeInTheDocument();
		await expect(canvas.getByText('done')).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: /Add note/ }));
		const titleInput = canvas.getByPlaceholderText('what this note is about');
		await userEvent.type(titleInput, 'Confirmed fix ships in the same PR');
		await userEvent.click(canvas.getByRole('button', { name: 'Save note' }));

		await waitFor(() =>
			expect(canvas.getByText('Confirmed fix ships in the same PR')).toBeInTheDocument()
		);
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			<SessionNotebook {...args} />
		</CollapseStoreProvider>
	{/snippet}
</Story>

<Story
	name="Error"
	args={{ sessionId: SESSION_ID }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			calls = 0;
			globalThis.fetch = (async () => {
				calls += 1;
				return new Response('down', { status: 503, statusText: 'Service Unavailable' });
			}) as typeof fetch;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: 'Notebook' }));

		await waitFor(() =>
			expect(canvas.getByText("Couldn't load this session's notebook")).toBeInTheDocument()
		);
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(1);

		await userEvent.click(canvas.getByRole('button', { name: /retry/i }));
		await waitFor(() => expect(calls).toBe(2));
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(2);
		await expect(canvas.getByText("Couldn't load this session's notebook")).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			<SessionNotebook {...args} />
		</CollapseStoreProvider>
	{/snippet}
</Story>
