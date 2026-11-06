<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import SessionMailboxPanel from './SessionMailboxPanel.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	import type { MailboxEnvelope, MailboxMessage } from '$lib/api/mailbox';
	import type { ThreadSummaryDto } from '$lib/comms/types';

	const STORY_SESSION = 'sess-aaaaaaaa-bbbb-cccc-dddd-000000000001';
	const PEER_A = 'sess-eeeeeeee-ffff-1111-2222-000000000002';
	const PEER_B = 'sess-99999999-8888-7777-6666-000000000003';
	const STATIC_NOW = new Date('2026-08-10T12:30:00Z').getTime();

	function msg(
		overrides: Partial<MailboxMessage> & { message_uuid: string; body: string }
	): MailboxMessage {
		return {
			from_session_uuid: null,
			to_session_uuid: STORY_SESSION,
			queued_at: '2026-08-10T12:00:00Z',
			claimed_at: null,
			sent_request_id: null,
			sent_at: null,
			consumed_at: null,
			dead_lettered_at: null,
			delivery_attempts: 0,
			status: 'pending',
			...overrides
		};
	}

	const ENVELOPE: MailboxEnvelope = {
		session_id: STORY_SESSION,
		inbox: [
			msg({
				message_uuid: 'm-1',
				from_session_uuid: PEER_A,
				body: 'Pending row.',
				status: 'pending'
			}),
			msg({
				message_uuid: 'm-2',
				from_session_uuid: PEER_B,
				body: 'Consumed row.',
				status: 'consumed',
				sent_request_id: 8001,
				sent_at: '2026-08-10T12:05:00Z',
				consumed_at: '2026-08-10T12:06:00Z'
			})
		],
		outbox: [
			msg({
				message_uuid: 'm-3',
				to_session_uuid: PEER_A,
				body: 'Delivered row.',
				status: 'delivered',
				sent_request_id: 8002,
				sent_at: '2026-08-10T12:10:00Z'
			})
		]
	};

	const THREADS: ThreadSummaryDto[] = [
		{
			thread_uuid: 'th-a',
			subject: 'Coordinate the mailbox.threads migration rollout',
			created_at: '2026-08-10T10:00:00Z',
			last_message_at: '2026-08-10T11:40:00Z',
			resolved_at: null,
			created_by_session_uuid: PEER_A,
			participants: [STORY_SESSION, PEER_A],
			tags: ['blocker'],
			message_count: 2,
			unread_count: 1,
			worst_delivery_state: 'delivered',
			has_stale_pending: false
		},
		{
			thread_uuid: 'th-b',
			subject: 'Frontend review notes',
			created_at: '2026-08-09T15:00:00Z',
			last_message_at: '2026-08-09T16:20:00Z',
			resolved_at: '2026-08-09T16:25:00Z',
			created_by_session_uuid: STORY_SESSION,
			participants: [STORY_SESSION, PEER_B],
			tags: ['review'],
			message_count: 2,
			unread_count: 0,
			worst_delivery_state: 'delivered',
			has_stale_pending: false
		}
	];

	function stubFetch(): () => void {
		const orig = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/threads')) {
				return new Response(JSON.stringify({ threads: THREADS, total_count: THREADS.length }), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response(JSON.stringify(ENVELOPE), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}) as typeof fetch;
		return () => {
			globalThis.fetch = orig;
		};
	}

	const { Story } = defineMeta({
		title: 'Shell/SessionMailboxPanel',
		component: SessionMailboxPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Summary"
	loaders={[
		async () => {
			localStorage.clear();
			const restore = stubFetch();
			return { restore };
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const header = canvas.getByRole('button', { name: /mailbox/i });
		await expect(header).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(header);
		await expect(header).toHaveAttribute('aria-expanded', 'true');

		await waitFor(() => expect(canvas.getByText('Waiting a while 1')).toBeInTheDocument());
		await expect(canvas.getByText('Read 1')).toBeInTheDocument();
		await expect(canvas.getByText('Delivered 1')).toBeInTheDocument();
		const staleBucket = canvas.getByText('Waiting a while 1');
		await expect(staleBucket).toHaveClass('badge-warning');

		await waitFor(() =>
			expect(
				canvas.getByText('Coordinate the mailbox.threads migration rollout')
			).toBeInTheDocument()
		);
		await expect(canvas.getByText('Frontend review notes')).toBeInTheDocument();

		const openComms = canvas.getByRole('link', { name: /Open comms/i });
		await expect(openComms).toHaveAttribute('href', `/sessions/${STORY_SESSION}/comms`);
	}}
>
	{#snippet template()}
		<div style="max-width: 760px; background: var(--surface-bg); padding: 16px;">
			<CollapseStoreProvider>
				<SessionMailboxPanel sessionId={STORY_SESSION} now={STATIC_NOW} />
			</CollapseStoreProvider>
		</div>
	{/snippet}
</Story>
