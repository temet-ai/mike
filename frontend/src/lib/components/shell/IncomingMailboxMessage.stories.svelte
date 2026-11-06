<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, waitFor, userEvent } from 'storybook/test';
	import IncomingMailboxMessage from './IncomingMailboxMessage.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import type { MailboxMessage } from '$lib/api/mailbox';

	const SESSION_ID = 'sess-11111111-2222-3333-4444-555555555555';
	const SENDER_ID = 'sess-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee';
	const STATIC_NOW = new Date('2026-07-02T15:00:00Z').getTime();

	const DELIVERED: MailboxMessage = {
		message_uuid: 'm-delivered-1',
		from_session_uuid: SENDER_ID,
		to_session_uuid: SESSION_ID,
		body:
			'Heads-up: the shared ledger helper moved to timelineTurnView.ts — rebase before you touch TurnCard.\n\n' +
			'Second line proves the full body is reachable after expand — never sliced.',
		queued_at: '2026-07-02T14:50:00Z',
		claimed_at: '2026-07-02T14:52:00Z',
		sent_request_id: 85102,
		sent_at: '2026-07-02T14:53:10Z',
		consumed_at: null,
		dead_lettered_at: null,
		delivery_attempts: 1,
		status: 'delivered'
	};

	const QUEUED: MailboxMessage = {
		message_uuid: 'm-queued-1',
		from_session_uuid: null,
		to_session_uuid: SESSION_ID,
		body: 'Reminder: run scripts/sb-test before you hand the timeline work back.',
		queued_at: '2026-07-02T14:59:00Z',
		claimed_at: null,
		sent_request_id: null,
		sent_at: null,
		consumed_at: null,
		dead_lettered_at: null,
		delivery_attempts: 0,
		status: 'pending'
	};

	const { Story } = defineMeta({
		title: 'Shell/IncomingMailboxMessage',
		component: IncomingMailboxMessage,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText('Incoming message')).toBeInTheDocument();
		await expect(canvas.getByText(/Heads-up: the shared ledger helper moved/)).toBeInTheDocument();
		await expect(canvas.queryByText(/Second line proves/)).not.toBeInTheDocument();
		await expect(canvas.getAllByText('click message to expand').length).toBe(2);

		const deliveredToggle = canvas.getByRole('button', { name: /expand incoming message/i });
		await expect(deliveredToggle).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(deliveredToggle);
		await expect(deliveredToggle).toHaveAttribute('aria-expanded', 'true');

		await waitFor(() =>
			expect(canvas.getByText(/Second line proves the full body is reachable/)).toBeInTheDocument()
		);

		await expect(canvas.getByText('Queued message')).toBeInTheDocument();
		await expect(canvas.getByText('waiting for next turn')).toBeInTheDocument();
		await expect(canvas.getByText(/run scripts\/sb-test/)).toBeInTheDocument();

		const deliveredCard = canvasElement.querySelector('.im-card--delivered');
		const queuedCard = canvasElement.querySelector('.im-card--queued');
		for (const card of [deliveredCard, queuedCard]) {
			if (!card) continue;
			const style = getComputedStyle(card);
			await expect(style.borderLeftWidth).toBe(style.borderTopWidth);
			await expect(style.borderLeftWidth).toBe(style.borderRightWidth);
		}
	}}
>
	{#snippet template()}
		<div
			style="max-width: 640px; background: var(--surface-bg); padding: 16px; display: flex; flex-direction: column; gap: 8px;"
		>
			<IncomingMailboxMessage
				message={DELIVERED}
				variant="delivered"
				sessionId={SESSION_ID}
				now={STATIC_NOW}
				turnIndex={2}
				query=""
			/>
			<IncomingMailboxMessage
				message={QUEUED}
				variant="queued"
				sessionId={SESSION_ID}
				now={STATIC_NOW}
				turnIndex={null}
				query=""
			/>
		</div>
	{/snippet}
</Story>
