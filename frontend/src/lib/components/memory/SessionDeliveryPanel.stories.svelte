<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import SessionDeliveryPanel from './SessionDeliveryPanel.svelte';
	import type { SessionDelivery } from '$lib/api/sessionDelivery';

	const { Story } = defineMeta({
		title: 'Memory/SessionDeliveryPanel',
		component: SessionDeliveryPanel,
		tags: ['autodocs']
	});

	const SESSION_ID = '9d9b690e-cb59-4607-81f0-c96bdb3ff9b2';

	const POPULATED: SessionDelivery = {
		session_uuid: SESSION_ID,
		generated_at: '2026-10-14T13:00:00Z',
		profile: {
			project: {
				id: 3,
				path: '/Users/dev/temet-proxy',
				name: 'temet-proxy',
				resolution: 'git'
			},
			first_seen: '2026-10-14T08:00:00Z',
			last_seen: '2026-10-14T12:00:00Z',
			models: [{ model: 'claude-opus-4', request_count: 120, requests_with_manifest: 118 }],
			agents: [
				{
					persona: 'Rust Development Expert',
					agent_name: 'rust-developer',
					first_request_id: 101,
					started_at: '2026-10-14T09:00:00Z'
				}
			],
			toggles: [
				{
					key: 'reminders',
					scope: 'session',
					applies: 'new_sessions',
					enabled: true,
					basis: 'session_frozen',
					enabled_now: true,
					changed_since_session_start: false
				},
				{
					key: 'recall_injector',
					scope: 'session',
					applies: 'new_sessions',
					enabled: false,
					basis: 'change_log_at_session_start',
					enabled_now: true,
					changed_since_session_start: true
				},
				{
					key: 'launch_brief',
					scope: 'global',
					applies: 'immediate',
					enabled: null,
					basis: 'change_log_at_session_start',
					enabled_now: true,
					changed_since_session_start: true
				}
			],
			limits: {
				inject_lessons: true,
				max_lessons_to_inject: 5,
				max_lessons_per_topic: 3,
				basis: 'current_settings'
			},
			toggle_changes: [
				{
					key: 'recall_injector',
					changed_at: '2026-10-14T10:30:00Z',
					enabled_before: false,
					enabled_after: true,
					via: 'spa',
					during_session: true
				}
			]
		},
		lessons: [
			{
				lesson_id: 68,
				text: 'Prefer scripts/cx over the manual .env source preamble for cargo invocations.',
				kind: 'convention',
				topic: null,
				stage: 'daily',
				archived_at: null,
				archived_reason: null,
				injection_count: 4,
				first_injected_at: '2026-10-14T09:00:00Z',
				last_injected_at: '2026-10-14T11:00:00Z',
				first_request_id: 101,
				last_request_id: 140,
				lanes: ['primary'],
				personas: ['Rust Development Expert']
			},
			{
				lesson_id: 70,
				text: 'Superseded advice about the shared target directory.',
				kind: 'correction',
				topic: 'cargo',
				stage: 'archived',
				archived_at: '2026-10-14T12:30:00Z',
				archived_reason: 'superseded',
				injection_count: 1,
				first_injected_at: '2026-10-14T09:05:00Z',
				last_injected_at: '2026-10-14T09:05:00Z',
				first_request_id: 102,
				last_request_id: 102,
				lanes: ['primary'],
				personas: []
			}
		],
		topics: [
			{
				topic: 'sqlx',
				state: 'recalled',
				recall_request_ids: [155],
				delivered_lesson_ids: [],
				available_project_lessons: 3,
				available_shared_lessons: 1
			},
			{
				topic: 'tokio',
				state: 'available',
				recall_request_ids: [],
				delivered_lesson_ids: [],
				available_project_lessons: 4,
				available_shared_lessons: 0
			}
		],
		recall_injections: [
			{ request_id: 155, recalled_at: '2026-10-14T10:40:00Z', topics: ['sqlx'], lesson_count: 2 }
		],
		skills: [
			{
				name: 'rust-lsp',
				state: 'loaded',
				load_count: 2,
				error_count: 0,
				first_request_id: 160,
				last_request_id: 190,
				first_loaded_at: '2026-10-14T10:50:00Z',
				last_loaded_at: '2026-10-14T11:30:00Z'
			},
			{
				name: 'no-such-skill',
				state: 'load_failed',
				load_count: 1,
				error_count: 1,
				first_request_id: 161,
				last_request_id: 161,
				first_loaded_at: '2026-10-14T10:51:00Z',
				last_loaded_at: '2026-10-14T10:51:00Z'
			}
		],
		recall_tool_calls: [
			{
				tool_name: 'mcp__mike__recall_project_context',
				call_count: 2,
				error_count: 0,
				first_request_id: 170,
				last_request_id: 180,
				first_called_at: '2026-10-14T11:00:00Z',
				last_called_at: '2026-10-14T11:10:00Z'
			}
		],
		injection_stages: [
			{
				stage: 'session_lessons',
				outcome: 'applied',
				reason_code: 'session_lessons_injected',
				request_count: 4,
				first_request_id: 101,
				last_request_id: 140
			},
			{
				stage: 'recall_injector',
				outcome: 'applied',
				reason_code: 'recall_lessons_injected',
				request_count: 1,
				first_request_id: 155,
				last_request_id: 155
			},
			{
				stage: 'read_dedup',
				outcome: 'unchanged',
				reason_code: 'read_dedup_no_duplicate',
				request_count: 110,
				first_request_id: 100,
				last_request_id: 220
			}
		],
		evidence_gaps: [
			{
				code: 'skill_selection_not_recorded',
				detail:
					'Mike does not record which skills were offered to the session or which package version was loaded. Only skills the session itself loaded are listed.'
			},
			{
				code: 'session_settings_not_in_memory',
				detail:
					"The proxy no longer holds this session's frozen settings, so toggle values come from the change log relative to the session's first request."
			}
		]
	};

	let calls = 0;

	function stubFetch(status: number, body: unknown): void {
		globalThis.fetch = (async () =>
			new Response(JSON.stringify(body), {
				status,
				headers: { 'content-type': 'application/json' }
			})) as typeof fetch;
	}
</script>

<script lang="ts">
	import { setContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';

	setContext(COLLAPSE_STORE_KEY, createCollapseStore());
</script>

<Story
	name="Populated"
	args={{ sessionId: SESSION_ID }}
	loaders={[
		async () => {
			localStorage.clear();
			stubFetch(200, POPULATED);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.queryByText(/Use scripts\/cx/)).not.toBeInTheDocument();
		await userEvent.click(canvas.getByRole('button', { name: /what mike delivered/i }));

		await waitFor(() => expect(canvas.getByText(/Prefer scripts\/cx/)).toBeInTheDocument());
		await expect(canvas.getByText('full text injected 4×')).toBeInTheDocument();
		await expect(canvas.getByText('now archived: superseded')).toBeInTheDocument();
		await expect(canvas.getByText('recalled on request')).toBeInTheDocument();
		await expect(canvas.getByText('available, no delivery recorded')).toBeInTheDocument();
		await expect(canvas.getByText('rust-lsp')).toBeInTheDocument();
		await expect(canvas.getByText('load failed')).toBeInTheDocument();
		await expect(canvas.getByText('unknown')).toBeInTheDocument();
		await expect(canvas.getAllByText(/changed since, now on/)).toHaveLength(2);
		await expect(canvas.getByText(/does not show that the model read it/)).toBeInTheDocument();
		await expect(canvas.getAllByText(/Mike does not record which skills/)).toHaveLength(1);
	}}
>
	{#snippet template(args)}
		<div style="max-width: 820px; padding: 12px;">
			<SessionDeliveryPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	args={{ sessionId: SESSION_ID }}
	loaders={[
		async () => {
			localStorage.clear();
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
		await userEvent.click(canvas.getByRole('button', { name: /what mike delivered/i }));

		await waitFor(() =>
			expect(canvas.getByText("Couldn't load what was delivered")).toBeInTheDocument()
		);
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(1);

		await userEvent.click(canvas.getByRole('button', { name: /retry/i }));
		await waitFor(() => expect(calls).toBe(2));
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(2);
		await expect(canvas.getByText("Couldn't load what was delivered")).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 820px; padding: 12px;">
			<SessionDeliveryPanel {...args} />
		</div>
	{/snippet}
</Story>
