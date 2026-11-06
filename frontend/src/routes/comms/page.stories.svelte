<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import type {
		ParticipantIdentityDto,
		ThreadMessageDto,
		ThreadSummaryDto
	} from '$lib/comms/types';

	const PROJECT_A = '/mnt/data/projects/temet/temet-proxy';
	const PROJECT_B = '/mnt/data/projects/other-repo';
	const PROJECT_C = '/mnt/data/projects/holdfast';
	const SESSION_A = 'aa1a2b3c-4d5e-6f70-8192-a3b4c5d6e7f8';
	const SESSION_B = 'bb1a2b3c-4d5e-6f70-8192-a3b4c5d6e7f8';
	const SESSION_C = 'cc1a2b3c-4d5e-6f70-8192-a3b4c5d6e7f8';

	const IDENTITIES: Record<string, ParticipantIdentityDto> = {
		[SESSION_A]: {
			session_uuid: SESSION_A,
			label: 'Migration coordinator',
			role: null,
			agent_type: null
		},
		[SESSION_B]: {
			session_uuid: SESSION_B,
			label: 'rust-developer',
			role: 'rust-developer',
			agent_type: null
		},
		[SESSION_C]: {
			session_uuid: SESSION_C,
			label: 'frontend-developer',
			role: null,
			agent_type: 'frontend-developer'
		}
	};
	const THREAD_1 = 'th-11111111-2222-3333-4444-555555555555';
	const THREAD_2 = 'th-66666666-7777-8888-9999-000000000000';
	const THREAD_3 = 'th-aaaaaaaa-7777-8888-9999-000000000000';
	const THREAD_4 = 'th-bbbbbbbb-7777-8888-9999-000000000000';
	const THREAD_5 = 'th-cccccccc-7777-8888-9999-000000000000';
	const FLEET_CR3 = 'fleet-cr3-0000-0000-0000-000000000000';
	const GOAL =
		'Everything milestone 25 still lists as not built, in order: the change record store, the what_changed answers, the Edit-error proof, the clone launcher refusal, and the handover note a successor reads first. Each seat takes one task, verifies it against the criteria, and records evidence before the coordinator closes the fleet. Nothing is clipped anywhere on this page, so this goal reads in full in the list row, the transcript heading and the goal paragraph, however long it runs.';
	const FLEET_SUBJECT = `Fleet change-record-3: ${GOAL}`;
	const CR3_ROLES = [
		'coordinator',
		'tooling-T11',
		'rust-developer',
		'rust-developer',
		'db-dev',
		'db-dev',
		'frontend-developer',
		'frontend-developer',
		'acceptance',
		'observer',
		'tooling-T12',
		'tooling-T13',
		'worker-4',
		'worker-5'
	];
	const CR3_IDENTITIES: ParticipantIdentityDto[] = CR3_ROLES.map((role, i) => {
		const short = (0xe74ba661 + i * 0x1111).toString(16).padStart(8, '0');
		return {
			session_uuid: `${short}-4d5e-6f70-8192-a3b4c5d6e7f8`,
			label: `${role}-${short}`,
			role: null,
			agent_type: null
		};
	});
	const CR3_COORDINATOR = CR3_IDENTITIES[0]!;
	const MIKE_DIGEST = Array.from(
		{ length: 36 },
		(_, i) => `Line ${i + 1}: seat ${i % 14} reports its task state and the evidence it recorded.`
	).join('\n');
	const STATIC_NOW = new Date('2026-08-10T12:30:00Z').getTime();

	function threadMsg(
		overrides: Partial<ThreadMessageDto> & {
			message_uuid: string;
			body: string;
			thread_uuid: string;
		}
	): ThreadMessageDto {
		return {
			from_session_uuid: null,
			to_session_uuid: SESSION_A,
			queued_at: '2026-08-10T10:00:00Z',
			claimed_at: null,
			sent_request_id: null,
			sent_at: null,
			consumed_at: null,
			dead_lettered_at: null,
			delivery_attempts: 0,
			status: 'delivered',
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
			created_by_session_uuid: SESSION_A,
			participants: [SESSION_A, SESSION_B],
			participant_identities: [IDENTITIES[SESSION_A]!, IDENTITIES[SESSION_B]!],
			tags: ['blocker'],
			message_count: 2,
			unread_count: 1,
			worst_delivery_state: 'delivered',
			has_stale_pending: false,
			project: { project_uuid: 'proj-a', name: 'mike', path: PROJECT_A, pinned: true },
			fleet: { fleet_uuid: 'fleet-1', slug: 'core-game', state: 'running' }
		},
		{
			thread_uuid: FLEET_CR3,
			subject: FLEET_SUBJECT,
			created_at: '2026-08-10T09:00:00Z',
			last_message_at: '2026-08-10T12:10:00Z',
			resolved_at: null,
			created_by_session_uuid: CR3_COORDINATOR.session_uuid,
			participants: CR3_IDENTITIES.map((i) => i.session_uuid),
			participant_identities: CR3_IDENTITIES,
			tags: [],
			message_count: 3,
			unread_count: 0,
			worst_delivery_state: 'delivered',
			has_stale_pending: false,
			project: { project_uuid: 'proj-a', name: 'mike', path: PROJECT_A, pinned: true },
			fleet: { fleet_uuid: FLEET_CR3, slug: 'change-record-3', state: 'running' }
		},
		{
			thread_uuid: THREAD_2,
			subject: 'Other-repo dependency bump review',
			created_at: '2026-08-09T15:00:00Z',
			last_message_at: '2026-08-09T16:20:00Z',
			resolved_at: null,
			created_by_session_uuid: SESSION_C,
			participants: [SESSION_C],
			participant_identities: [IDENTITIES[SESSION_C]!],
			tags: ['review'],
			message_count: 1,
			unread_count: 0,
			worst_delivery_state: 'delivered',
			has_stale_pending: false,
			project: { project_uuid: 'proj-b', name: PROJECT_B, path: PROJECT_B },
			fleet: null
		},
		{
			thread_uuid: THREAD_3,
			subject: 'Plain thread without a fleet',
			created_at: '2026-08-10T11:00:00Z',
			last_message_at: '2026-08-10T12:00:00Z',
			resolved_at: null,
			created_by_session_uuid: SESSION_C,
			participants: [SESSION_C],
			participant_identities: [IDENTITIES[SESSION_C]!],
			tags: [],
			message_count: 5,
			unread_count: 0,
			worst_delivery_state: 'delivered',
			has_stale_pending: false,
			project: { project_uuid: 'proj-a', name: 'mike', path: PROJECT_A, pinned: true },
			fleet: null
		},
		{
			thread_uuid: THREAD_5,
			subject: 'Holdfast tuning notes',
			created_at: '2026-08-08T10:00:00Z',
			last_message_at: '2026-08-08T11:00:00Z',
			resolved_at: null,
			created_by_session_uuid: SESSION_C,
			participants: [SESSION_C],
			participant_identities: [IDENTITIES[SESSION_C]!],
			tags: [],
			message_count: 2,
			unread_count: 0,
			worst_delivery_state: 'delivered',
			has_stale_pending: false,
			project: { project_uuid: 'proj-c', name: PROJECT_C, path: PROJECT_C },
			fleet: null
		},
		{
			thread_uuid: THREAD_4,
			subject: 'Thread from a session with no project',
			created_at: '2026-08-10T12:10:00Z',
			last_message_at: '2026-08-10T12:20:00Z',
			resolved_at: null,
			created_by_session_uuid: SESSION_C,
			participants: [SESSION_C],
			participant_identities: [IDENTITIES[SESSION_C]!],
			tags: [],
			message_count: 3,
			unread_count: 0,
			worst_delivery_state: 'delivered',
			has_stale_pending: false,
			project: null,
			fleet: null
		}
	];

	const MESSAGES_BY_THREAD = new Map<string, ThreadMessageDto[]>([
		[
			THREAD_1,
			[
				threadMsg({
					message_uuid: 'm-t1-0001',
					thread_uuid: THREAD_1,
					from_session_uuid: SESSION_B,
					to_session_uuid: SESSION_A,
					body: 'Opening this thread to coordinate the mailbox.threads migration rollout.',
					sent_request_id: 91001
				}),
				threadMsg({
					message_uuid: 'm-t1-0002',
					thread_uuid: THREAD_1,
					in_reply_to_uuid: 'm-t1-0001',
					from_session_uuid: SESSION_A,
					to_session_uuid: SESSION_B,
					body: 'Backfill is running now.'
				})
			]
		],
		[
			FLEET_CR3,
			[
				threadMsg({
					message_uuid: 'm-cr3-0001',
					thread_uuid: FLEET_CR3,
					from_session_uuid: null,
					to_session_uuid: CR3_COORDINATOR.session_uuid,
					body: `mike digest: Fleet change-record-3 at 53s\n${MIKE_DIGEST}`,
					sent_request_id: 92001
				}),
				threadMsg({
					message_uuid: 'm-cr3-0002',
					thread_uuid: FLEET_CR3,
					from_session_uuid: CR3_COORDINATOR.session_uuid,
					to_session_uuid: CR3_IDENTITIES[1]!.session_uuid,
					queued_at: '2026-08-10T10:30:00Z',
					body: 'Take the Edit-error proof next.'
				})
			]
		],
		[
			THREAD_2,
			[
				threadMsg({
					message_uuid: 'm-t2-0001',
					thread_uuid: THREAD_2,
					from_session_uuid: SESSION_C,
					to_session_uuid: SESSION_C,
					body: 'Reviewed the dependency bump — looks safe to merge.'
				})
			]
		]
	]);

	const pinState: Record<string, boolean> = {};
	let slowThreads = false;

	function fleetProjects(): FleetProjectWithCounts[] {
		const base = [
			{
				project_path: PROJECT_A,
				project_uuid: 'proj-a',
				project_id: 1,
				custom_name: 'mike',
				pinned: true
			},
			{
				project_path: PROJECT_B,
				project_uuid: 'proj-b',
				project_id: 2,
				custom_name: null,
				pinned: false
			},
			{
				project_path: PROJECT_C,
				project_uuid: 'proj-c',
				project_id: 3,
				custom_name: null,
				pinned: false
			}
		];
		return base.map((p) => ({
			...p,
			pinned: pinState[p.project_path] ?? p.pinned,
			session_count: 1,
			request_count: 1,
			first_seen: '2026-08-01T00:00:00Z',
			last_seen: '2026-08-10T12:00:00Z'
		}));
	}

	function installGlobalCommsFetchStub(): () => void {
		const orig = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			const method = (init?.method ?? 'GET').toUpperCase();
			const json = (body: unknown, status = 200): Response =>
				new Response(JSON.stringify(body), {
					status,
					headers: { 'content-type': 'application/json' }
				});

			if (method === 'GET' && slowThreads && url.includes('/api/threads')) {
				await new Promise((r) => setTimeout(r, 400));
			}
			if (method === 'GET' && url.includes('/api/threads?')) {
				const qs = new URL(url, 'http://localhost').searchParams;
				const tag = qs.get('tag');
				const participant = qs.get('participant');
				const resolved = qs.get('resolved');
				const filtered = THREADS.filter((t) => {
					if (tag && !t.tags.includes(tag)) return false;
					if (participant && !t.participants.includes(participant)) return false;
					if (resolved === 'true' && t.resolved_at === null) return false;
					if (resolved === 'false' && t.resolved_at !== null) return false;
					return true;
				});
				return json({ threads: filtered, total_count: filtered.length });
			}
			if (method === 'GET' && url.includes('/api/projects')) return json(fleetProjects());
			const pinMatch = url.match(/\/api\/projects\/([^/?]+)\/pin$/);
			if (method === 'PATCH' && pinMatch) {
				const path = decodeURIComponent(pinMatch[1] ?? '');
				const pinned = (JSON.parse((init?.body as string) ?? '{}') as { pinned: boolean }).pinned;
				pinState[path] = pinned;
				return json({ project_path: path, pinned });
			}
			if (method === 'GET' && url.endsWith('/api/threads')) {
				return json({ threads: THREADS, total_count: THREADS.length });
			}
			if (method === 'GET' && url.includes('/api/tags')) {
				return json({
					tags: [
						{ code: 'blocker', hue: 5, usage_count: 2 },
						{ code: 'review', hue: 8, usage_count: 1 }
					]
				});
			}
			const threadMatch = url.match(/\/api\/threads\/([^/?]+)$/);
			if (method === 'GET' && threadMatch) {
				const uuid = decodeURIComponent(threadMatch[1] ?? '');
				const thread = THREADS.find((t) => t.thread_uuid === uuid);
				const messages = MESSAGES_BY_THREAD.get(uuid) ?? [];
				if (!thread) return json({ error: 'not found' }, 404);
				return json({ thread, messages });
			}
			const resolveMatch = url.match(/\/api\/threads\/([^/?]+)\/resolve$/);
			if (method === 'POST' && resolveMatch) {
				const uuid = decodeURIComponent(resolveMatch[1] ?? '');
				const resolvedAt = new Date(STATIC_NOW).toISOString();
				const thread = THREADS.find((t) => t.thread_uuid === uuid);
				if (thread) thread.resolved_at = resolvedAt;
				return json({ thread_uuid: uuid, resolved_at: resolvedAt });
			}

			return json({ error: 'unhandled in story stub', url, method }, 404);
		}) as typeof fetch;
		return () => {
			globalThis.fetch = orig;
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Comms',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			const restore = installGlobalCommsFetchStub();
			return { restore };
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						`http://localhost/comms?p=${encodeURIComponent(PROJECT_A)}&thread=${FLEET_CR3}`
					),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const projectRow = (path: string) =>
			canvasElement.querySelector(
				`[data-slot="project-row"][data-project-path="${path}"]`
			) as HTMLElement;
		const rowButton = (subject: string) =>
			canvas
				.getByText(subject, { selector: '.thread-row__subject' })
				.closest('.thread-row-wrap')
				?.querySelector('.thread-row') as HTMLElement;

		await waitFor(() =>
			expect(
				canvas.getByText('Coordinate the mailbox.threads migration rollout')
			).toBeInTheDocument()
		);
		await waitFor(() => expect(canvasElement.querySelector('.transcript__goal')).not.toBeNull());
		await expect(canvasElement.querySelector('.transcript__goal')?.textContent).toBe(GOAL);
		await expect(canvasElement.querySelector('[data-slot="projects-pinned"]')).not.toBeNull();
		await expect(projectRow(PROJECT_A)).toHaveAttribute('aria-expanded', 'true');
		await expect(projectRow(PROJECT_B)).toHaveAttribute('aria-expanded', 'false');
		await expect(projectRow(PROJECT_A).textContent).toContain('3 threads');
		await expect(projectRow(PROJECT_A).textContent).toContain(PROJECT_A);
		await expect(projectRow(PROJECT_A).textContent).toContain('updated');
		await expect(canvas.getByText('No project')).toBeInTheDocument();
		await expect(
			canvasElement.querySelector('[data-slot="filter-scope-status"]')?.textContent
		).toContain('every project · 1 pinned');

		await expect(canvas.getByText('fleet core-game')).toBeInTheDocument();
		await expect(canvas.getByText('5 messages')).toBeInTheDocument();
		const rowSubjects = Array.from(canvasElement.querySelectorAll('.thread-row__subject')).map(
			(e) => e.textContent
		);
		await expect(rowSubjects.findIndex((t) => t?.startsWith('Fleet change-record-3'))).toBeLessThan(
			rowSubjects.indexOf('Plain thread without a fleet')
		);
		const ownRow = canvasElement
			.querySelector('.thread-row__subject .badge-nx, .thread-row__subject [data-slot="badge"]')
			?.closest('.thread-row-wrap') as HTMLElement;
		await expect(ownRow.querySelector('.thread-row__subject')?.textContent).toContain(GOAL);
		await expect(within(ownRow).getAllByText(/change-record-3/).length).toBe(1);
		await expect(within(ownRow).getAllByRole('link', { name: /Open comms for/ }).length).toBe(14);
		await expect(
			Math.ceil(ownRow.querySelector('.thread-row__subject')!.getBoundingClientRect().height)
		).toBeGreaterThan(40);

		const search = canvas.getByRole('searchbox', { name: 'Search projects by name or path' });
		await userEvent.type(search, 'other-repo');
		await waitFor(() => {
			expect(
				canvas.queryByRole('link', { name: /Open comms for Migration coordinator/ })
			).toBeNull();
			expect(projectRow(PROJECT_A)).toBeNull();
			expect(projectRow(PROJECT_B)).not.toBeNull();
		});
		await userEvent.clear(search);
		await waitFor(() => expect(projectRow(PROJECT_A)).not.toBeNull());
		await userEvent.type(search, 'no-such-project');
		await waitFor(() =>
			expect(canvas.getByText('No projects match "no-such-project"')).toBeInTheDocument()
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Clear filters' }));
		await waitFor(() => expect(projectRow(PROJECT_A)).not.toBeNull());
		await expect((search as HTMLInputElement).value).toBe('');

		const pinB = within(projectRow(PROJECT_B).closest('.pt-row') as HTMLElement).getByRole(
			'button',
			{ name: 'Pin project' }
		);
		await userEvent.click(pinB);

		await waitFor(() =>
			expect(canvas.getAllByRole('button', { name: 'Unpin project' }).length).toBe(2)
		);
		await userEvent.click(canvas.getAllByRole('button', { name: 'Unpin project' }).at(-1)!);

		await waitFor(() =>
			expect(
				within(projectRow(PROJECT_B).closest('.pt-row') as HTMLElement).getByRole('button', {
					name: 'Pin project'
				})
			).toBeInTheDocument()
		);

		const folder = canvas.getByRole('button', { name: 'Collapse folder mnt/data/projects' });
		await expect(projectRow(PROJECT_C)).not.toBeNull();
		await userEvent.click(folder);

		await waitFor(() => expect(projectRow(PROJECT_C)).toBeNull());
		await userEvent.click(canvas.getByRole('button', { name: 'Expand folder mnt/data/projects' }));

		await waitFor(() => expect(projectRow(PROJECT_C)).not.toBeNull());

		await expect(projectRow(PROJECT_B).tagName).toBe('BUTTON');
		await expect(projectRow(PROJECT_B).hasAttribute('href')).toBe(false);
		await expect(
			within(projectRow(PROJECT_B).closest('.pt-row') as HTMLElement).getByRole('link', {
				name: 'Open project other-repo'
			})
		).toHaveAttribute('href', `/projects/${encodeURIComponent(PROJECT_B)}`);
		const startUrl = window.location.href;
		await userEvent.click(projectRow(PROJECT_B));

		await waitFor(() => {
			expect(projectRow(PROJECT_B)).toHaveAttribute('aria-expanded', 'true');
			expect(canvas.getByText('Other-repo dependency bump review')).toBeInTheDocument();
			expect(canvas.queryByText('Coordinate the mailbox.threads migration rollout')).toBeNull();
		});
		await expect(window.location.href).toBe(startUrl);
		await userEvent.click(projectRow(PROJECT_B));

		await waitFor(() => {
			expect(projectRow(PROJECT_B)).toHaveAttribute('aria-expanded', 'false');
			expect(canvas.queryByText('Other-repo dependency bump review')).toBeNull();
		});
		await userEvent.pointer(
			{ keys: '[MouseLeft]', target: projectRow(PROJECT_A) },
			{ pointerEventsCheck: 0 }
		);

		await waitFor(() =>
			expect(
				canvas.getByText('Coordinate the mailbox.threads migration rollout')
			).toBeInTheDocument()
		);
		await waitFor(() => expect(canvasElement.querySelectorAll('.pt-expanded').length).toBe(1));
		await new Promise((r) => setTimeout(r, 300));
		within(canvasElement.querySelector('[data-slot="projects-pinned"]') as HTMLElement)
			.getByRole('button', { name: 'Edit project' })
			.click();
		await waitFor(() => expect(within(document.body).getByRole('dialog')).toBeInTheDocument());
		await userEvent.keyboard('{Escape}');

		await waitFor(() => {
			expect(within(document.body).queryByRole('dialog')).toBeNull();
			expect(getComputedStyle(document.body).pointerEvents).not.toBe('none');
		});

		await expect(projectRow(PROJECT_A)).toHaveAttribute('aria-expanded', 'true');
		await userEvent.click(rowButton('Plain thread without a fleet'));

		await waitFor(() => expect(canvasElement.querySelector('.transcript')).not.toBeNull());
		slowThreads = true;
		await userEvent.click(canvas.getByRole('button', { name: 'Refresh' }));

		await new Promise((r) => setTimeout(r, 100));
		await expect(canvasElement.querySelector('.project-list__skeleton')).toBeNull();
		await expect(projectRow(PROJECT_A)).toHaveAttribute('aria-expanded', 'true');
		await expect(
			canvas.getByText('Plain thread without a fleet', { selector: '.thread-row__subject' })
		).toBeInTheDocument();
		slowThreads = false;
		await new Promise((r) => setTimeout(r, 450));

		const participantLinks = canvas.getAllByRole('link', { name: /Open comms for/i });
		await expect(participantLinks.length).toBeGreaterThan(0);
		for (const link of participantLinks) {
			await expect(link.getAttribute('href')).toMatch(/\/sessions\/.+\/comms$/);
		}
		await expect(canvas.getByText('Migration coordinator')).toBeInTheDocument();
		await expect(canvas.getAllByText('rust-developer').length).toBeGreaterThan(0);

		const ownRowNow = canvasElement
			.querySelector('.thread-row__subject .badge-nx, .thread-row__subject [data-slot="badge"]')
			?.closest('.thread-row-wrap') as HTMLElement;
		await userEvent.click(ownRowNow.querySelector('.thread-row') as HTMLElement);
		await waitFor(() => expect(canvasElement.querySelector('.transcript__goal')).not.toBeNull());
		await expect(
			canvas.getByRole('heading', { level: 2, name: 'Fleet change-record-3' })
		).toBeInTheDocument();
		await expect(canvasElement.querySelector('.transcript__goal')?.textContent).toBe(GOAL);
		await expect(canvasElement.querySelector('.transcript__meta')?.textContent).toContain(
			'Reply from'
		);
		const mikeMsg = canvasElement.querySelector('[data-message-uuid="m-cr3-0001"]') as HTMLElement;
		await expect(mikeMsg.querySelector('.transcript-msg__sender')?.textContent?.trim()).toBe(
			'mike'
		);
		await expect(mikeMsg.querySelector('.comms-avatar')).toBeTruthy();
		const mikeBody = mikeMsg.querySelector('.transcript-msg__body') as HTMLElement;
		await expect(mikeBody.scrollHeight).toBe(mikeBody.clientHeight);
		await expect(mikeBody.textContent).toContain(MIKE_DIGEST);
		const toHead = mikeMsg.querySelector('.transcript-msg__head')?.textContent ?? '';
		await expect(toHead.match(/e74ba661/g)?.length ?? 0).toBe(1);
		await expect(canvasElement.querySelector('.transcript-msg__meta')).toBeNull();

		await userEvent.click(rowButton('Coordinate the mailbox.threads migration rollout'));
		await waitFor(() =>
			expect(
				canvas.getByText('Opening this thread to coordinate the mailbox.threads migration rollout.')
			).toBeInTheDocument()
		);
		await expect(canvasElement.querySelector('.transcript-compose')).toBeNull();
		await expect(canvas.queryByRole('button', { name: /^Reply$/i })).toBeNull();

		const senderLink = canvasElement.querySelector(
			'[data-message-uuid="m-t1-0001"] .transcript-msg__sender'
		) as HTMLAnchorElement;
		await expect(senderLink.tagName).toBe('A');
		await expect(senderLink.getAttribute('href')).toBe(`/sessions/${SESSION_B}/comms`);
		const senderUuidSpan = senderLink.querySelector('.transcript-msg__uuid');
		await expect(senderUuidSpan).toBeTruthy();
		const senderLabelText = (senderLink.textContent ?? '')
			.replace(senderUuidSpan?.textContent ?? '', '')
			.trim();
		await expect(senderLabelText).toBe('rust-developer');

		await expect(canvasElement.textContent?.toLowerCase()).not.toMatch(/\bunknown\b/);

		const resolveBtn = canvas.getByRole('button', { name: 'Resolve thread' });
		await userEvent.click(resolveBtn);
		await waitFor(() => expect(canvas.getAllByText(/resolved/).length).toBeGreaterThan(0));
	}}
>
	{#snippet template()}
		<div class="w-[1200px] bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
