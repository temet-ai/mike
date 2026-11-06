<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import CommsRouteHarness from './__harness__/CommsRouteHarness.svelte';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import type { OperatorTurnDto, SessionDetail, SessionSummary } from '$lib/types';
	import type { ThreadMessageDto, ThreadSummaryDto } from '$lib/comms/types';

	const STORY_SESSION_ID = 'cc1a2b3c-4d5e-6f70-8192-a3b4c5d6e7f8';
	const PEER_A = 'aa1a2b3c-4d5e-6f70-8192-a3b4c5d6e7f8';
	const PEER_B = 'bb1a2b3c-4d5e-6f70-8192-a3b4c5d6e7f8';
	const THREAD_1 = 'th-11111111-2222-3333-4444-555555555555';
	const THREAD_2 = 'th-66666666-7777-8888-9999-000000000000';
	const STATIC_NOW = new Date('2026-08-10T12:30:00Z').getTime();

	function buildDetailStore(): SessionDetailStore {
		const summary: SessionSummary = {
			id: STORY_SESSION_ID,
			project: '/Users/vimes/projects/temet/temet-proxy',
			project_id: 1,
			cwd: '/Users/vimes/projects/temet/temet-proxy',
			project_resolution: 'git',
			first_seen: '2026-08-10T09:00:00Z',
			last_seen: '2026-08-10T12:29:00Z',
			request_count: 42,
			models: ['claude-opus-4-8'],
			last_status: 'Complete',
			title: 'Mailbox thread migration',
			pinned: false
		};
		const detail: SessionDetail = {
			summary,
			requests: [],
			total_requests: 0,
			offset: 0,
			limit: 50
		};
		return {
			get detail() {
				return detail;
			},
			get loading() {
				return false;
			},
			get loadingOlder() {
				return false;
			},
			get error() {
				return null;
			},
			get loadOlderError() {
				return null;
			},
			get notFound() {
				return false;
			},
			get hasOlder() {
				return false;
			},
			get olderCeilingReached() {
				return false;
			},
			get currentId() {
				return STORY_SESSION_ID;
			},
			get details() {
				return new Map();
			},
			get detailErrors() {
				return new Set<number>();
			},
			load: async () => {},
			refresh: async () => {},
			loadOlder: async () => {},
			loadAll: async () => {},
			loadDetail: async () => {},
			cancelDetail: () => {},
			predecessorOf: () => null,
			get operatorTurns() {
				return [] as OperatorTurnDto[];
			},
			get promptKindFilter() {
				return 'operator';
			},
			refreshOperatorTurns: async () => {},
			setPromptKindFilter: () => {},
			handleEvent: () => {}
		};
	}

	function threadMsg(
		overrides: Partial<ThreadMessageDto> & {
			message_uuid: string;
			body: string;
			thread_uuid: string;
		}
	): ThreadMessageDto {
		return {
			from_session_uuid: null,
			to_session_uuid: STORY_SESSION_ID,
			queued_at: '2026-08-10T10:00:00Z',
			claimed_at: null,
			sent_request_id: null,
			sent_at: null,
			consumed_at: null,
			dead_lettered_at: null,
			delivery_attempts: 0,
			status: 'pending',
			in_reply_to_uuid: null,
			context_items: [],
			...overrides
		};
	}

	const THREADS: ThreadSummaryDto[] = [
		{
			thread_uuid: THREAD_1,
			subject: 'Coordinate the mailbox.threads migration rollout',
			created_at: '2026-08-10T10:00:00Z',
			last_message_at: '2026-08-10T11:40:00Z',
			resolved_at: null,
			created_by_session_uuid: PEER_A,
			participants: [STORY_SESSION_ID, PEER_A],
			tags: ['blocker', 'db'],
			message_count: 4,
			unread_count: 1,
			worst_delivery_state: 'dead_lettered',
			has_stale_pending: false
		},
		{
			thread_uuid: THREAD_2,
			subject: 'Frontend review notes for the comms page',
			created_at: '2026-08-09T15:00:00Z',
			last_message_at: '2026-08-09T16:20:00Z',
			resolved_at: '2026-08-09T16:25:00Z',
			created_by_session_uuid: STORY_SESSION_ID,
			participants: [STORY_SESSION_ID, PEER_B],
			tags: ['review'],
			message_count: 2,
			unread_count: 0,
			worst_delivery_state: 'delivered',
			has_stale_pending: false
		}
	];

	const THREAD_1_MESSAGES: ThreadMessageDto[] = [
		threadMsg({
			message_uuid: 'm-t1-0001',
			thread_uuid: THREAD_1,
			from_session_uuid: PEER_A,
			to_session_uuid: STORY_SESSION_ID,
			body:
				'Opening this thread to coordinate the mailbox.threads migration rollout — full body, must render ' +
				'without any clamp:\n\n1. Backfill mailbox.messages.thread_uuid for existing rows.\n' +
				'2. Ship GET /api/sessions/{id}/threads behind the same auth as /messages.\n' +
				'3. Frontend consumes it read-only until the reply endpoint lands.',
			queued_at: '2026-08-10T10:00:00Z',
			claimed_at: '2026-08-10T10:01:00Z',
			sent_request_id: 91001,
			sent_at: '2026-08-10T10:01:30Z',
			status: 'delivered',
			delivery_attempts: 1
		}),
		threadMsg({
			message_uuid: 'm-t1-0002',
			thread_uuid: THREAD_1,
			in_reply_to_uuid: 'm-t1-0001',
			from_session_uuid: STORY_SESSION_ID,
			to_session_uuid: PEER_A,
			body: 'Reply attempted 5 times and exceeded the redelivery cap — dead-lettered, needs a manual re-send.',
			queued_at: '2026-08-10T11:35:00Z',
			claimed_at: '2026-08-10T11:36:00Z',
			dead_lettered_at: '2026-08-10T11:40:00Z',
			status: 'dead_lettered',
			delivery_attempts: 5
		}),
		threadMsg({
			message_uuid: 'm-t1-0003',
			thread_uuid: THREAD_1,
			in_reply_to_uuid: 'm-t1-0002',
			from_session_uuid: PEER_A,
			to_session_uuid: STORY_SESSION_ID,
			body: 'Re-sent manually — see the tracking issue and the backfill migration file.',
			queued_at: '2026-08-10T11:50:00Z',
			claimed_at: '2026-08-10T11:51:00Z',
			sent_request_id: 91010,
			sent_at: '2026-08-10T11:51:20Z',
			status: 'delivered',
			delivery_attempts: 1,
			context_items: [
				{ kind: 'url', value: 'https://github.com/temet-ai/mike/issues/133', label: 'issue #133' },
				{ kind: 'file', value: 'crates/mike/src/store/persistence/mailbox.rs', label: null }
			]
		}),
		threadMsg({
			message_uuid: 'm-t1-0004',
			thread_uuid: THREAD_1,
			in_reply_to_uuid: 'm-t1-0003',
			from_session_uuid: STORY_SESSION_ID,
			to_session_uuid: PEER_A,
			body: 'Confirmed delivered — closing the loop on this sub-thread.',
			queued_at: '2026-08-10T12:00:00Z',
			claimed_at: '2026-08-10T12:01:00Z',
			status: 'claimed',
			delivery_attempts: 1
		})
	];

	const THREAD_2_MESSAGES: ThreadMessageDto[] = [
		threadMsg({
			message_uuid: 'm-t2-0001',
			thread_uuid: THREAD_2,
			from_session_uuid: STORY_SESSION_ID,
			to_session_uuid: PEER_B,
			body: 'Kicking off the comms-page review thread.',
			queued_at: '2026-08-09T15:00:00Z',
			claimed_at: '2026-08-09T15:01:00Z',
			sent_request_id: 90501,
			sent_at: '2026-08-09T15:01:20Z',
			status: 'consumed',
			consumed_at: '2026-08-09T15:10:00Z',
			delivery_attempts: 1
		}),
		threadMsg({
			message_uuid: 'm-t2-0002',
			thread_uuid: THREAD_2,
			in_reply_to_uuid: 'm-t2-0001',
			from_session_uuid: PEER_B,
			to_session_uuid: STORY_SESSION_ID,
			body: 'Reviewed — looks good, resolving.',
			queued_at: '2026-08-09T16:10:00Z',
			claimed_at: '2026-08-09T16:11:00Z',
			sent_request_id: 90512,
			sent_at: '2026-08-09T16:11:30Z',
			status: 'delivered',
			delivery_attempts: 1
		})
	];

	const TEAM_DISPATCHES = [
		{
			agent_label: 'rust-developer',
			description: 'Backfilling mailbox.messages.thread_uuid for the threads migration',
			first_seen: '2026-08-10T09:05:00Z',
			last_seen: '2026-08-10T12:10:00Z',
			request_count: 14
		}
	];
	const TEAM_MESSAGES = [
		{
			ts: '2026-08-10T09:10:00Z',
			direction: 'sent' as const,
			counterpart: 'rust-developer',
			summary: null,
			body: 'Kicking off the rollout coordination — start with the backfill migration.'
		},
		{
			ts: '2026-08-10T11:55:00Z',
			direction: 'received' as const,
			counterpart: 'rust-developer',
			summary: 'Backfill done, GET route live',
			body: 'Backfill migration landed and GET /api/sessions/{id}/threads is live behind the same auth as /messages.'
		}
	];

	interface FixtureState {
		threads: ThreadSummaryDto[];
		messagesByThread: Map<string, ThreadMessageDto[]>;
	}

	function makeFixtureState(): FixtureState {
		return {
			threads: [],
			messagesByThread: new Map([
				[THREAD_1, THREAD_1_MESSAGES.map((m) => ({ ...m }))],
				[THREAD_2, THREAD_2_MESSAGES.map((m) => ({ ...m }))]
			])
		};
	}

	function stubCommsFetch(fx: FixtureState): () => void {
		const orig = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			const method = (init?.method ?? 'GET').toUpperCase();
			const json = (body: unknown, status = 200): Response =>
				new Response(JSON.stringify(body), {
					status,
					headers: { 'content-type': 'application/json' }
				});

			if (method === 'GET' && url.includes(`/api/sessions/${STORY_SESSION_ID}/threads`)) {
				return json({ threads: fx.threads, total_count: fx.threads.length });
			}
			if (method === 'GET' && url.includes(`/api/sessions/${STORY_SESSION_ID}/fleet`)) {
				return json({
					in_fleet: true,
					peer_count: 2,
					message_count: 6,
					fleet_size: 6,
					window_hours: 24
				});
			}
			if (method === 'GET' && url.includes('/api/tags')) {
				return json({
					tags: [
						{ code: 'blocker', hue: 5, usage_count: 3 },
						{ code: 'db', hue: 2, usage_count: 1 },
						{ code: 'review', hue: 8, usage_count: 4 }
					]
				});
			}
			if (method === 'GET' && url.includes(`/api/sessions/${STORY_SESSION_ID}/messages`)) {
				const allMessages = Array.from(fx.messagesByThread.values()).flat();
				const inbox = allMessages.filter((m) => m.to_session_uuid === STORY_SESSION_ID);
				const outbox = allMessages.filter((m) => m.from_session_uuid === STORY_SESSION_ID);
				return json({ session_id: STORY_SESSION_ID, inbox, outbox });
			}
			if (method === 'GET' && url.includes(`/api/sessions/${STORY_SESSION_ID}/team-comms`)) {
				return json({ dispatches: TEAM_DISPATCHES, messages: TEAM_MESSAGES });
			}
			if (method === 'GET' && url.includes('/api/fleet/graph')) {
				return json({
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					generated_at: '2026-08-10T12:30:00Z',
					nodes: [
						{
							session_id: PEER_A,
							title: 'Migration coordinator',
							agent_type: null,
							role: 'rust-developer',
							model: 'claude-opus-4-8',
							workspace: null,
							cwd: null,
							worktree: null,
							goal: null,
							next_steps: null,
							state: 'working',
							last_activity_at: '2026-08-10T12:20:00Z',
							request_count: 12,
							run_id: null
						},
						{
							session_id: PEER_B,
							title: 'Frontend reviewer',
							agent_type: 'frontend-developer',
							role: null,
							model: 'claude-sonnet-4-6',
							workspace: null,
							cwd: null,
							worktree: null,
							goal: null,
							next_steps: null,
							state: 'idle',
							last_activity_at: '2026-08-09T16:30:00Z',
							request_count: 8,
							run_id: null
						}
					],
					edges: []
				});
			}
			const threadMatch = url.match(/\/api\/threads\/([^/?]+)$/);
			if (method === 'GET' && threadMatch) {
				const uuid = decodeURIComponent(threadMatch[1] ?? '');
				const thread = fx.threads.find((t) => t.thread_uuid === uuid);
				const messages = fx.messagesByThread.get(uuid) ?? [];
				if (!thread) return json({ error: 'not found' }, 404);
				return json({ thread, messages });
			}
			const replyMatch = url.match(/\/api\/threads\/([^/?]+)\/message$/);
			if (method === 'POST' && replyMatch) {
				const uuid = decodeURIComponent(replyMatch[1] ?? '');
				const sent = init?.body
					? (JSON.parse(String(init.body)) as {
							from_session_id: string | null;
							body: string;
							in_reply_to?: string;
							context?: ThreadMessageDto['context_items'];
						})
					: { from_session_id: null, body: '' };
				const thread = fx.threads.find((t) => t.thread_uuid === uuid);
				const otherParticipants = (thread?.participants ?? []).filter(
					(p) => p !== sent.from_session_id
				);
				const list = fx.messagesByThread.get(uuid) ?? [];
				const deliveries = otherParticipants.map((to, i) => {
					const row = threadMsg({
						message_uuid: `m-reply-${list.length + i}`,
						thread_uuid: uuid,
						from_session_uuid: sent.from_session_id,
						to_session_uuid: to,
						body: sent.body,
						queued_at: new Date(STATIC_NOW).toISOString(),
						status: 'pending',
						in_reply_to_uuid: sent.in_reply_to ?? null,
						context_items: sent.context ?? []
					});
					list.push(row);
					return { message_uuid: row.message_uuid, to_session_uuid: to };
				});
				fx.messagesByThread.set(uuid, list);
				if (thread) {
					thread.message_count += deliveries.length;
					thread.last_message_at = new Date(STATIC_NOW).toISOString();
				}
				return json({ thread_uuid: uuid, subject: thread?.subject ?? '', deliveries });
			}
			const resolveMatch = url.match(/\/api\/threads\/([^/?]+)\/resolve$/);
			if (method === 'POST' && resolveMatch) {
				const uuid = decodeURIComponent(resolveMatch[1] ?? '');
				const resolvedAt = new Date(STATIC_NOW).toISOString();
				const thread = fx.threads.find((t) => t.thread_uuid === uuid);
				if (thread) thread.resolved_at = resolvedAt;
				return json({ thread_uuid: uuid, resolved_at: resolvedAt });
			}
			const ackMatch = url.match(/\/api\/sessions\/[^/]+\/messages\/([^/?]+)\/ack$/);
			if (method === 'POST' && ackMatch) {
				const uuid = decodeURIComponent(ackMatch[1] ?? '');
				const consumedAt = new Date(STATIC_NOW).toISOString();
				for (const list of fx.messagesByThread.values()) {
					const row = list.find((m) => m.message_uuid === uuid);
					if (row) {
						row.status = 'consumed';
						row.consumed_at = consumedAt;
					}
				}
				return json({ message_uuid: uuid, consumed_at: consumedAt });
			}
			if (method === 'POST' && url.endsWith('/api/threads')) {
				const sent = init?.body
					? (JSON.parse(String(init.body)) as {
							subject: string;
							body: string;
							from_session_id: string | null;
							project_path: string | null;
							tags?: string[];
							notify?: string[];
						})
					: { subject: '', body: '', from_session_id: null, project_path: null };
				const from = sent.from_session_id ?? STORY_SESSION_ID;
				const notify = sent.notify ?? [];
				const newUuid = `th-open-${fx.threads.length}`;
				const nowIso = new Date(STATIC_NOW).toISOString();
				fx.threads.push({
					thread_uuid: newUuid,
					subject: sent.subject,
					created_at: nowIso,
					last_message_at: nowIso,
					resolved_at: null,
					created_by_session_uuid: from,
					participants: [from, ...notify],
					tags: sent.tags ?? [],
					message_count: 1,
					unread_count: 0,
					worst_delivery_state: 'pending',
					has_stale_pending: false
				});
				fx.messagesByThread.set(newUuid, [
					threadMsg({
						message_uuid: `m-${newUuid}-0001`,
						thread_uuid: newUuid,
						from_session_uuid: from,
						to_session_uuid: from,
						body: sent.body,
						queued_at: nowIso,
						status: 'pending'
					})
				]);
				return json({ thread_uuid: newUuid, subject: sent.subject }, 201);
			}
			const newThreadMatch = url.match(/\/api\/sessions\/([^/?]+)\/message$/);
			if (method === 'POST' && newThreadMatch) {
				const to = decodeURIComponent(newThreadMatch[1] ?? '');
				const sent = init?.body
					? (JSON.parse(String(init.body)) as {
							from_session_id?: string;
							message: string;
							subject: string;
							tags?: string[];
						})
					: { message: '', subject: '' };
				const from = sent.from_session_id ?? STORY_SESSION_ID;
				const newUuid = `th-new-${fx.threads.length}`;
				fx.threads.push({
					thread_uuid: newUuid,
					subject: sent.subject,
					created_at: new Date(STATIC_NOW).toISOString(),
					last_message_at: new Date(STATIC_NOW).toISOString(),
					resolved_at: null,
					created_by_session_uuid: from,
					participants: [from, to],
					tags: sent.tags ?? [],
					message_count: 1,
					unread_count: 0,
					worst_delivery_state: 'pending',
					has_stale_pending: false
				});
				fx.messagesByThread.set(newUuid, [
					threadMsg({
						message_uuid: `m-${newUuid}-0001`,
						thread_uuid: newUuid,
						from_session_uuid: from,
						to_session_uuid: to,
						body: sent.message,
						queued_at: new Date(STATIC_NOW).toISOString(),
						status: 'pending'
					})
				]);
				return json({
					message_uuid: `m-${newUuid}-0001`,
					to_session_uuid: to,
					from_session_uuid: from,
					queued_at: new Date(STATIC_NOW).toISOString()
				});
			}

			return json({ error: 'unhandled in story stub', url, method }, 404);
		}) as typeof fetch;
		return () => {
			globalThis.fetch = orig;
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Sessions/Comms',
		component: CommsRouteHarness,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ detailStore: buildDetailStore() }}
	loaders={[
		async () => {
			const fx = makeFixtureState();
			const restore = stubCommsFetch(fx);
			return { restore, fx };
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}/comms`),
					params: { sessionId: STORY_SESSION_ID }
				}
			}
		}
	}}
	play={async ({ canvasElement, loaded }) => {
		const canvas = within(canvasElement);
		const fx = loaded.fx as FixtureState;

		await waitFor(() => expect(canvas.getByText(/peers/)).toBeInTheDocument());

		const peerBand = canvasElement.querySelector('.comms-peer-band') as HTMLElement;
		await waitFor(() => expect(within(peerBand).getByText('rust-developer')).toBeInTheDocument());
		await expect(within(peerBand).getByText('frontend-developer')).toBeInTheDocument();

		await waitFor(() => expect(canvas.getByText(/This session coordinates/)).toBeInTheDocument());
		await expect(canvas.getByText(/No mailbox threads yet/)).toBeInTheDocument();

		await expect(
			canvas.getByText('Backfilling mailbox.messages.thread_uuid for the threads migration')
		).toBeInTheDocument();
		await expect(canvas.getByText(/Kicking off the rollout coordination/)).toBeInTheDocument();
		await expect(canvas.getByText('Backfill done, GET route live')).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: /New thread/i }));
		const form = canvasElement.querySelector('.new-thread__form') as HTMLElement;
		await expect(within(form).queryByText('To')).toBeNull();
		const notifyDetails = form.querySelector('.notify-picker') as HTMLDetailsElement;
		await expect(notifyDetails.open).toBe(false);

		const subjectInput = form.querySelector('input[type="text"]') as HTMLInputElement;
		await userEvent.type(subjectInput, 'Kickoff: onboarding checklist');
		const newBody = form.querySelector('textarea') as HTMLTextAreaElement;
		await userEvent.type(
			newBody,
			'Starting a fresh thread from the comms page — no recipient required.'
		);

		fx.threads.push(...THREADS.map((t) => ({ ...t })));

		await userEvent.click(canvas.getByRole('button', { name: 'Start thread' }));

		await waitFor(() => expect(canvas.queryByText(/This session coordinates/)).toBeNull());
		await waitFor(() =>
			expect(canvas.getAllByText('Kickoff: onboarding checklist').length).toBe(2)
		);
		await expect(
			canvas.getByText('Starting a fresh thread from the comms page — no recipient required.')
		).toBeInTheDocument();
		const deliveryStoppedLine = canvas.getByText('Delivery stopped');
		await expect(deliveryStoppedLine.closest('.thread-row-wrap')).toHaveClass(
			'thread-row-wrap--warn'
		);
		await expect(canvas.getByText('resolved')).toBeInTheDocument();

		const rail = canvasElement.querySelector('.comms-body__list') as HTMLElement;
		await expect(getComputedStyle(rail).position).toBe('sticky');
		const teamFeed = canvasElement.querySelector('#comms-team-feed') as HTMLElement;
		await expect(getComputedStyle(teamFeed).overflowY).toBe('visible');
		const peerStrip = canvasElement.querySelector('.peer-strip') as HTMLElement;
		await expect(getComputedStyle(peerStrip).flexWrap).toBe('nowrap');
		await expect(getComputedStyle(peerStrip).overflowX).toBe('auto');

		await userEvent.click(canvas.getByText('Coordinate the mailbox.threads migration rollout'));
		await waitFor(() => expect(canvas.getByText(/without any clamp/)).toBeInTheDocument());
		const selectedWrap = canvasElement.querySelector('.thread-row-wrap--selected') as HTMLElement;
		await expect(selectedWrap).toHaveAttribute('data-augmented-ui', 'br-clip border');
		await expect(canvas.getByRole('link', { name: '#91001' })).toBeVisible();
		await expect(canvas.getByTitle(/5 attempt/)).toBeInTheDocument();

		const depth3 = canvasElement.querySelector('[data-depth="3"]') as HTMLElement | null;
		await expect(depth3).toBeTruthy();
		await expect(depth3?.dataset.messageUuid).toBe('m-t1-0004');
		await expect(depth3?.title).toContain('depth 3');

		const issueChip = canvas.getByRole('link', { name: /issue #133/i });
		await expect(issueChip).toHaveAttribute('href', 'https://github.com/temet-ai/mike/issues/133');
		await expect(issueChip).toHaveAttribute('target', '_blank');
		await expect(
			canvas.getByText('crates/mike/src/store/persistence/mailbox.rs')
		).toBeInTheDocument();

		const rootRow = canvasElement.querySelector('[data-message-uuid="m-t1-0001"]') as HTMLElement;
		await userEvent.click(within(rootRow).getByRole('button', { name: /reply/i }));
		await waitFor(() => expect(canvasElement.querySelector('.reply-target-chip')).toBeTruthy());
		const replyChip = canvasElement.querySelector('.reply-target-chip') as HTMLElement;
		await expect(replyChip.textContent).toContain('Replying to');
		await expect(replyChip.textContent).toContain('rust-developer');
		await userEvent.click(replyChip.querySelector('.reply-target-chip__cancel') as HTMLElement);
		await waitFor(() => expect(canvasElement.querySelector('.reply-target-chip')).toBeNull());

		const ackButtons = canvas.getAllByRole('button', { name: /^Ack/ });
		await expect(ackButtons.length).toBeGreaterThan(0);
		await userEvent.click(ackButtons[0]!);
		await waitFor(() => expect(canvas.getAllByText('Read').length).toBeGreaterThan(0));

		const textarea = canvasElement.querySelector(
			'.transcript-compose__textarea'
		) as HTMLTextAreaElement;
		await userEvent.click(textarea);
		await userEvent.type(textarea, 'Re-sending now that the cap reset.');
		await userEvent.keyboard('{Enter}');
		await waitFor(() =>
			expect(canvas.getByText('Re-sending now that the cap reset.')).toBeInTheDocument()
		);

		const resolveBtn = canvas.getByRole('button', { name: 'Resolve thread' });
		await userEvent.click(resolveBtn);
		await waitFor(() => expect(canvas.getAllByText(/resolved/).length).toBeGreaterThan(1));
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px] bg-[var(--surface-bg)]">
			<CommsRouteHarness {...args} />
		</div>
	{/snippet}
</Story>
