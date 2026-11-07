<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import SessionMemoryActions from './SessionMemoryActions.svelte';
	import type { ConsolidationAction } from '$lib/api/memoryDecisions';
	import type { LessonDto } from '$lib/types';

	const { Story } = defineMeta({
		title: 'Memory/SessionMemoryActions',
		component: SessionMemoryActions,
		tags: ['autodocs']
	});

	const SESSION_ID = '00000000-0000-4000-8000-000000000001';

	const ACTIONS: ConsolidationAction[] = [
		{
			id: 68,
			run_id: 12,
			run_kind: 'session',
			lesson_id: 68,
			lesson_uuid: '018f7b2a-0000-7000-8000-000000000068',
			project_id: 3,
			project_path: '/Users/dev/temet-proxy',
			session_id: 501,
			session_uuid: SESSION_ID,
			source_turn_id: 812,
			files_touched: ['crates/mike/src/store/persistence/lessons.rs'],
			agent_id: 4,
			lesson_text:
				'FIXED 2026-07-19: the seeded DB credential was postgres:postgres — corrected and confirmed the migration runner connects.',
			action_kind: 'archived_duplicate',
			outcome: 'applied',
			error: null,
			before_stage: 'daily',
			after_stage: 'archived',
			decided_by: 'embedding_distance',
			distance: 0.0421,
			threshold: 0.85,
			embedding_model: 'text-embedding-3-small',
			counterpart_lesson_id: 66,
			counterpart_uuid: '018f7b2a-0000-7000-8000-000000000066',
			counterpart_text: 'DB credential appears broken in local dev — needs investigation.',
			runners_up: [{ lesson_id: 61, distance: 0.79 }],
			rank_detail: null,
			injection_count: 4,
			created_at: '2026-07-19T09:13:00Z'
		},
		{
			id: 70,
			run_id: 12,
			run_kind: 'session',
			lesson_id: 70,
			lesson_uuid: '018f7b2a-0000-7000-8000-000000000070',
			project_id: 3,
			project_path: '/Users/dev/temet-proxy',
			session_id: 501,
			session_uuid: SESSION_ID,
			source_turn_id: 820,
			files_touched: null,
			agent_id: null,
			lesson_text:
				'Prefer `scripts/cx` over the manual `.env` source preamble for cargo invocations.',
			action_kind: 'promoted_to_daily',
			outcome: 'applied',
			error: null,
			before_stage: 'raw',
			after_stage: 'daily',
			decided_by: null,
			distance: null,
			threshold: null,
			embedding_model: null,
			counterpart_lesson_id: null,
			counterpart_uuid: null,
			counterpart_text: null,
			runners_up: [],
			rank_detail: null,
			injection_count: 0,
			created_at: '2026-07-28T02:00:00Z'
		}
	];

	function jsonResponse(body: unknown): Promise<Response> {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	let calls = 0;

	function stubFetch(): void {
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url =
				typeof input === 'string'
					? input
					: input instanceof URL
						? input.href
						: (input as Request).url;
			if (url.includes('/memory-actions')) {
				return jsonResponse(ACTIONS);
			}
			if (url.includes('/restore')) {
				const restored: LessonDto = {
					id: 68,
					project_id: 3,
					project_path: '/Users/dev/temet-proxy',
					kind: 'long_term',
					text: ACTIONS[0]!.lesson_text,
					source_turn_id: 812,
					created_at: '2026-07-19T09:13:00Z',
					expires_at: null,
					topic: null
				};
				return jsonResponse(restored);
			}
			return jsonResponse({});
		}) as typeof fetch;
	}
</script>

<Story
	name="Default"
	args={{ sessionId: SESSION_ID }}
	loaders={[
		async () => {
			stubFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.queryByText(ACTIONS[0]!.lesson_text)).not.toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: /memory actions/i }));
		await waitFor(() => expect(canvas.getByText(ACTIONS[0]!.lesson_text)).toBeInTheDocument());
		await expect(canvas.getByText(ACTIONS[1]!.lesson_text)).toBeInTheDocument();
		await expect(canvas.getByText('2')).toBeInTheDocument();

		const restoreButton = canvas.getByRole('button', { name: /restore this lesson/i });
		await userEvent.click(restoreButton);
		await waitFor(() =>
			expect(canvas.queryByRole('button', { name: /restore this lesson/i })).not.toBeInTheDocument()
		);
	}}
>
	{#snippet template(args)}
		<div style="max-width: 760px; padding: 12px;">
			<SessionMemoryActions {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	args={{ sessionId: SESSION_ID }}
	loaders={[
		async () => {
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
		await userEvent.click(canvas.getByRole('button', { name: /memory actions/i }));

		await waitFor(() =>
			expect(canvas.getByText("Couldn't load memory actions")).toBeInTheDocument()
		);
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(1);

		await userEvent.click(canvas.getByRole('button', { name: /retry/i }));
		await waitFor(() => expect(calls).toBe(2));
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(2);
		await expect(canvas.getByText("Couldn't load memory actions")).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 760px; padding: 12px;">
			<SessionMemoryActions {...args} />
		</div>
	{/snippet}
</Story>
