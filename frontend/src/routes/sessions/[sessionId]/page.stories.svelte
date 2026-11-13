<script module lang="ts">
	import { stubTimeSpentFetch } from '$lib/components/__fixtures__/timeSpent';
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import SessionDetailRouteHarness from './__harness__/SessionDetailRouteHarness.svelte';
	import type {
		OperatorTurnDto,
		RequestSummary,
		SessionDetail,
		SessionSummary,
		ToolUseDto,
		ToolUsesResponse
	} from '$lib/types';
	import {
		stubBandBodyFetch,
		stubSessionOverviewFetch
	} from '$lib/components/__fixtures__/bandBodies';
	import { buildMockDetailStore, buildMockSessionListStore } from '$lib/test-support';
	import { closeDrawer, shellState } from '$lib/stores/shellState.svelte';
	import { createSessionFlowStore } from '$lib/session-flow/sessionFlowStore.svelte';
	import { createSessionFlowLiveAdapter } from '$lib/session-flow/liveAdapter';
	import { ProxyEventStream } from '$lib/ws.svelte';
	import type { FetchSessionFlowPage } from '$lib/api/sessionFlow';
	import type { SessionFlowPage } from '$lib/api/generated/SessionFlowPage';
	import type { SessionFlowActor } from '$lib/api/generated/SessionFlowActor';
	import type { SessionFlowActivity } from '$lib/api/generated/SessionFlowActivity';
	import {
		flowActor,
		flowActivity,
		flowCoverage,
		flowTime
	} from '$lib/session-flow/__fixtures__/sessionFlow';

	const STORY_SESSION_ID = 'f1c8a2d0-7b4e-4a90-b3c2-9d5e8a1f6c20';
	const SESSION_FIRST_SEEN = '2026-06-26T17:18:00Z';
	const SESSION_LAST_SEEN = '2026-06-27T19:11:00Z';
	const KICKOFF_PROMPT =
		'The http://localhost:9990/settings page has a lot of settings that are no ' +
		'longer consistent. We need to break the settings page into more settings ' +
		'components. First target is to move the workers to a separate page — steerers, ' +
		'background workers, the "LLM model global" toggle. The OpenRouter API key is ' +
		'below the steerer toggle, which is wrong. We need a reusable component for ' +
		'defining a worker/agent: the top shows what the worker is, then lets the user ' +
		'enable/disable it; if disabled, the model selection collapses.';
	const STATIC_NOW = new Date('2026-06-27T19:12:00Z').getTime();

	function buildSummary(overrides: Partial<SessionSummary> = {}): SessionSummary {
		return {
			id: STORY_SESSION_ID,
			project: '/Users/vimes/projects/temet/temet-proxy',
			project_id: 1,
			cwd: '/Users/vimes/projects/temet/temet-proxy',
			project_resolution: 'git',
			first_seen: SESSION_FIRST_SEEN,
			last_seen: SESSION_LAST_SEEN,
			request_count: 279,
			models: ['claude-opus-4-8', 'claude-sonnet-4-6', 'claude-haiku-4-5'],
			last_status: 'Complete',
			title: 'Reorganize settings nav and routes',
			pinned: false,
			...overrides
		};
	}

	function buildRequest(overrides: {
		id: number;
		status?: string;
		model?: string;
		created_at?: string;
		completed_at?: string | null;
		path?: string;
		method?: string;
		message_count?: number;
		turn_origin?: RequestSummary['turn_origin'];
		subagent_persona?: string | null;
		human_prompt?: string;
	}): RequestSummary {
		return {
			id: overrides.id,
			method: overrides.method ?? 'POST',
			path: overrides.path ?? '/v1/messages',
			model: overrides.model ?? 'claude-opus-4-8',
			status: overrides.status ?? 'Complete',
			is_streaming: overrides.status === 'Streaming',
			message_count: overrides.message_count ?? 4,
			max_tokens: null,
			created_at: overrides.created_at ?? '2026-06-26T17:18:00Z',
			completed_at:
				overrides.completed_at !== undefined ? overrides.completed_at : '2026-06-26T17:18:25Z',
			session_id: STORY_SESSION_ID,
			project: '/Users/vimes/projects/temet/temet-proxy',
			human_prompt: overrides.human_prompt ?? KICKOFF_PROMPT,
			turn_type: 'user_prompt',
			turn_origin: overrides.turn_origin ?? 'primary',
			subagent_persona: overrides.subagent_persona ?? null,
			response_tool_count: 0,
			response_tools_used: [],
			tool_names: []
		};
	}

	function buildDetail(
		requests: RequestSummary[],
		overrides: Partial<SessionDetail> = {}
	): SessionDetail {
		return {
			summary: buildSummary({ request_count: requests.length }),
			requests,
			total_requests: requests.length,
			offset: 0,
			limit: 50,
			...overrides
		};
	}

	const DEFAULT_REQUESTS: RequestSummary[] = [
		buildRequest({
			id: 201,
			message_count: 4,
			created_at: '2026-06-26T17:18:00Z',
			completed_at: '2026-06-26T17:18:24Z'
		}),
		buildRequest({
			id: 207,
			model: 'claude-sonnet-4-6',
			message_count: 14,
			created_at: '2026-06-26T18:02:00Z',
			completed_at: '2026-06-26T18:02:27Z',
			human_prompt:
				'Move the steerer + background-worker toggles out of the main settings page into /settings/workers.'
		}),
		buildRequest({
			id: 213,
			message_count: 26,
			created_at: '2026-06-26T19:40:00Z',
			completed_at: '2026-06-26T19:40:23Z',
			human_prompt:
				'Build the reusable WorkerCard component — header, enable/disable, collapsing model select.'
		}),
		buildRequest({
			id: 221,
			model: 'claude-haiku-4-5',
			message_count: 32,
			created_at: '2026-06-26T20:15:00Z',
			completed_at: '2026-06-26T20:15:22Z',
			turn_origin: 'title_sidecar',
			human_prompt: '(title sidecar)'
		}),
		buildRequest({
			id: 234,
			message_count: 48,
			turn_origin: 'subagent',
			subagent_persona: 'Frontend Development Expert',
			created_at: '2026-06-27T09:30:00Z',
			completed_at: '2026-06-27T09:30:26Z',
			human_prompt: 'Build WorkerCard component + story'
		}),
		buildRequest({
			id: 248,
			message_count: 60,
			created_at: '2026-06-27T11:05:00Z',
			completed_at: '2026-06-27T11:05:25Z',
			human_prompt:
				'The OpenRouter API key field is below the steerer toggle — move it into the steerer card where it belongs.'
		}),
		buildRequest({
			id: 259,
			model: 'claude-sonnet-4-6',
			message_count: 72,
			created_at: '2026-06-27T14:20:00Z',
			completed_at: '2026-06-27T14:20:24Z',
			human_prompt:
				'Wire the new /settings/workers route into the settings nav and add the LlmModelGlobalControl.'
		}),
		buildRequest({
			id: 268,
			message_count: 84,
			created_at: '2026-06-27T16:48:00Z',
			completed_at: '2026-06-27T16:48:28Z',
			human_prompt:
				'Update the style guide with the WorkerCard pattern and the collapsing-model-select behaviour.'
		}),
		buildRequest({
			id: 275,
			message_count: 92,
			created_at: '2026-06-27T18:30:00Z',
			completed_at: '2026-06-27T18:30:23Z',
			human_prompt:
				'Add the settings-workers migration and the crates/mike settings module that backs the toggles.'
		}),
		buildRequest({
			id: 279,
			status: 'Streaming',
			message_count: 98,
			created_at: '2026-06-27T19:11:00Z',
			completed_at: null,
			human_prompt:
				'Run the frontend gate and confirm the workers page renders the WorkerCards with collapsing model selects.'
		})
	];

	const SETTINGS_WRITE_FILES: string[] = [
		'frontend/src/routes/settings/+page.svelte',
		'frontend/src/routes/settings/+layout.svelte',
		'frontend/src/routes/settings/workers/+page.svelte',
		'frontend/src/routes/settings/workers/+page.ts',
		'frontend/src/lib/components/settings/WorkerCard.svelte',
		'frontend/src/lib/components/settings/WorkerCard.stories.svelte',
		'frontend/src/lib/components/settings/SteererToggle.svelte',
		'frontend/src/lib/components/settings/BackgroundWorkerSection.svelte',
		'frontend/src/lib/components/settings/LlmModelGlobalControl.svelte',
		'frontend/src/lib/components/settings/OpenRouterKeyField.svelte',
		'frontend/src/lib/components/settings/SettingsNav.svelte',
		'frontend/src/lib/api/settings.ts',
		'frontend/src/lib/stores/settings.svelte.ts',
		'frontend/src/lib/types.ts',
		'docs/frontend/STYLE_GUIDE.md',
		'docs/frontend/style/component-patterns.md',
		'crates/mike-base/src/settings/mod.rs',
		'crates/mike/src/settings/workers.rs',
		'crates/mike/src/settings/llm_model.rs',
		'crates/mike/src/api/handlers.rs',
		'migrations/20260627090000_settings_workers.sql',
		'frontend/src/routes/settings/workers/page.test.ts'
	];

	function buildSettingsToolUses(sessionId: string): ToolUsesResponse {
		const rows: ToolUseDto[] = SETTINGS_WRITE_FILES.map((file_path, i) => ({
			request_id: 213 + i,
			session_id: sessionId,
			created_at: '2026-06-27T09:31:00Z',
			tool_id: `toolu_settings_write_${i.toString().padStart(3, '0')}`,
			tool_name: file_path.endsWith('.svelte') || file_path.endsWith('.ts') ? 'Edit' : 'Write',
			input: { file_path },
			file_path,
			ord: 0
		}));
		rows.push({
			request_id: 234,
			session_id: sessionId,
			created_at: '2026-06-27T09:30:00Z',
			tool_id: 'toolu_settings_agent_001',
			tool_name: 'Agent',
			input: {
				subagent_type: 'frontend-developer',
				description: 'Build WorkerCard component + story'
			},
			file_path: null,
			ord: 0
		});
		return { session_id: sessionId, total_count: rows.length, offset: 0, limit: 1000, rows };
	}

	const OUTCOME_MIX: OperatorTurnDto['outcome'][] = [
		'accepted',
		'accepted',
		'accepted',
		'accepted',
		'accepted',
		'accepted',
		'corrected',
		'corrected',
		'corrected',
		'corrected',
		'corrected',
		'corrected',
		'abandoned',
		'abandoned',
		'unknown'
	];

	const TURN_FILES_TOUCHED: string[] = [
		'frontend/src/routes/settings/+page.svelte',
		'frontend/src/routes/settings/workers/+page.svelte',
		'frontend/src/lib/components/settings/WorkerCard.svelte'
	];

	function buildSettingsOperatorTurns(sessionId: string): OperatorTurnDto[] {
		return OUTCOME_MIX.map((outcome, i) => ({
			turn_id: i + 1,
			session_id: sessionId,
			project_id: 1,
			start_request_id: 201 + i * 5,
			end_request_id: 205 + i * 5,
			start_request_number: 1 + i * 5,
			end_request_number: 5 + i * 5,
			user_prompt: `Settings-refactor turn ${i + 1}: split a settings concern into its own component/route.`,
			final_assistant_text: `Completed turn ${i + 1} of the settings reorganization.`,
			tool_call_count: 8,
			tool_error_count: 0,
			files_touched: TURN_FILES_TOUCHED,
			tools_used: ['Read', 'Edit', 'Write'],
			agents_dispatched: i === 4 ? [{ subagent_type: 'frontend-developer', count: 1 }] : [],
			weasel_findings_count: 0,
			total_input_tokens: 42000,
			total_output_tokens: 3800,
			duration_ms: 132000,
			outcome,
			outcome_evidence: null,
			classified_at: '2026-06-27T19:11:00Z',
			created_at: '2026-06-26T17:18:00Z',
			updated_at: '2026-06-27T19:11:00Z'
		}));
	}

	const DEFAULT_OPERATOR_TURNS = buildSettingsOperatorTurns(STORY_SESSION_ID);

	function buildFlowFetchPage(sessionId: string): FetchSessionFlowPage {
		return async ({ component }) => {
			const primary = component === 'requests';
			const page: SessionFlowPage = {
				version: 1,
				session_id: sessionId,
				availability: {
					eligible: { capture_version: 1, admitted_at: flowTime(0), producer: 'claude_code' }
				},
				actors: primary
					? [
							flowActor('root', {
								kind: 'session',
								session_id: sessionId,
								label: 'Reorganize settings nav and routes'
							})
						]
					: [],
				activities: primary
					? [
							flowActivity('requests:201:started', {
								actor_id: 'root',
								occurred_at: flowTime(0),
								ended_at: flowTime(24000),
								source: {
									table: 'requests',
									row_id: '201',
									session_id: sessionId,
									request_id: '201',
									detail_href: '/api/requests/201'
								}
							})
						]
					: [],
				relationships: [],
				coverage: flowCoverage({ component, exhausted: true }),
				next_cursor: null
			};
			return page;
		};
	}

	const PINNED_DETAIL = buildDetail(DEFAULT_REQUESTS, {
		summary: buildSummary({ pinned: true, request_count: 279 }),
		total_requests: 279
	});

	const SCALE_WORKER_COUNT = 32;
	const scaleRecoveryRecovered = { current: false };

	function buildScaleWorkers(
		sessionId: string,
		count: number
	): { actors: SessionFlowActor[]; activities: SessionFlowActivity[] } {
		const actors = Array.from({ length: count }, (_, i) =>
			flowActor(`worker-${i}`, {
				session_id: sessionId,
				label: `Worker ${i}`,
				producer_id: `worker-${i}`
			})
		);
		const activities = Array.from({ length: count }, (_, i) =>
			flowActivity(`requests:${300 + i}:started`, {
				actor_id: `worker-${i}`,
				occurred_at: flowTime(0),
				ended_at: flowTime(24000),
				source: {
					table: 'requests',
					row_id: String(300 + i),
					session_id: sessionId,
					request_id: String(300 + i),
					detail_href: `/api/requests/${300 + i}`
				}
			})
		);
		return { actors, activities };
	}

	function buildScaleRecoveryFlowFetchPage(sessionId: string): FetchSessionFlowPage {
		return async ({ component }) => {
			const primary = component === 'requests';
			const workerCount = scaleRecoveryRecovered.current
				? SCALE_WORKER_COUNT + 1
				: SCALE_WORKER_COUNT;
			const { actors, activities } = buildScaleWorkers(sessionId, workerCount);
			const page: SessionFlowPage = {
				version: 1,
				session_id: sessionId,
				availability: {
					eligible: { capture_version: 1, admitted_at: flowTime(0), producer: 'claude_code' }
				},
				actors: primary
					? [
							flowActor('root', {
								kind: 'session',
								session_id: sessionId,
								label: 'Reorganize settings nav and routes'
							}),
							...actors
						]
					: [],
				activities: primary ? activities : [],
				relationships: [],
				coverage: flowCoverage({ component, exhausted: true }),
				next_cursor: null
			};
			return page;
		};
	}

	const scaleRecoveryFlowStore = createSessionFlowStore({
		fetchPage: buildScaleRecoveryFlowFetchPage(STORY_SESSION_ID)
	});

	const { Story } = defineMeta({
		title: 'Routes/Sessions/Detail',
		component: SessionDetailRouteHarness,
		tags: ['autodocs'],
		argTypes: {
			detailStore: { control: false },
			sessionStore: { control: false }
		}
	});
</script>

<Story
	name="Default"
	args={{
		detailStore: buildMockDetailStore({
			detail: buildDetail(DEFAULT_REQUESTS, {
				summary: buildSummary({ request_count: 279 }),
				total_requests: 279
			}),
			hasOlder: true,
			operatorTurns: DEFAULT_OPERATOR_TURNS
		}),
		sessionStore: buildMockSessionListStore({
			sessions: [buildSummary({ title: 'Reorganize settings nav and routes' })]
		}),
		flowStore: createSessionFlowStore({ fetchPage: buildFlowFetchPage(STORY_SESSION_ID) })
	}}
	loaders={[
		async () => {
			stubSessionOverviewFetch({
				sessionId: STORY_SESSION_ID,
				requests: DEFAULT_REQUESTS,
				kickoffRequestId: DEFAULT_REQUESTS[0]!.id,
				kickoffUserPrompt: KICKOFF_PROMPT,
				summary: buildSummary({ request_count: 279 }),
				toolUses: buildSettingsToolUses(STORY_SESSION_ID),
				operatorTurns: DEFAULT_OPERATOR_TURNS
			});
			stubTimeSpentFetch();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`),
					params: { sessionId: STORY_SESSION_ID }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { name: 'Requests' })).toBeVisible();
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-slot="time-spent-summary"]')?.textContent).toBe(
				'108 h model · 116 h waiting'
			)
		);
		await expect(
			canvas.getByText('Session Flow loads once this section scrolls into view.')
		).toBeInTheDocument();

		const turn201 = canvasElement.querySelector('[data-band-id="201"]');
		if (!turn201) throw new Error('Turn card for the kickoff request (201) did not render');
		await userEvent.click(
			within(turn201 as HTMLElement).getByRole('button', { name: 'view request' })
		);

		await waitFor(() => {
			expect(shellState.drawerBandId).toBe(201);
			expect(shellState.drawerKind).toBe('request');
		});

		await waitFor(() => expect(canvas.getByText('Session flow')).toBeInTheDocument());
		await waitFor(() => expect(canvas.getByText('Request')).toBeInTheDocument());

		await expect(canvas.getByRole('heading', { name: 'Requests' })).toBeVisible();

		closeDrawer();
		await waitFor(() =>
			expect(within(document.body).queryByRole('dialog')).not.toBeInTheDocument()
		);
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionDetailRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Empty"
	args={{
		detailStore: buildMockDetailStore({ detail: buildDetail([]) }),
		sessionStore: buildMockSessionListStore({
			sessions: [buildSummary({ pinned: false, title: null })]
		})
	}}
	loaders={[
		async () => {
			stubBandBodyFetch();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`),
					params: { sessionId: STORY_SESSION_ID }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionDetailRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Loading"
	args={{
		detailStore: buildMockDetailStore({ detail: null, loading: true }),
		sessionStore: buildMockSessionListStore({
			sessions: [buildSummary({ pinned: false, title: null })]
		})
	}}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`),
					params: { sessionId: STORY_SESSION_ID }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionDetailRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	args={{
		detailStore: buildMockDetailStore({
			detail: null,
			error: 'Failed to fetch session: connection refused'
		}),
		sessionStore: buildMockSessionListStore({
			sessions: [buildSummary({ pinned: false, title: null })]
		})
	}}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`),
					params: { sessionId: STORY_SESSION_ID }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionDetailRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="NotFound"
	args={{
		detailStore: buildMockDetailStore({ detail: null, notFound: true }),
		sessionStore: buildMockSessionListStore({
			sessions: [buildSummary({ pinned: false, title: null })]
		})
	}}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`),
					params: { sessionId: STORY_SESSION_ID }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionDetailRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Pinned"
	args={{
		detailStore: buildMockDetailStore({ detail: PINNED_DETAIL }),
		sessionStore: buildMockSessionListStore({
			sessions: [buildSummary({ pinned: true, title: 'Auth module test suite' })]
		})
	}}
	loaders={[
		async () => {
			stubBandBodyFetch();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`),
					params: { sessionId: STORY_SESSION_ID }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionDetailRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="FlowScaleRecovery"
	args={{
		detailStore: buildMockDetailStore({
			detail: buildDetail(DEFAULT_REQUESTS, {
				summary: buildSummary({ request_count: DEFAULT_REQUESTS.length }),
				total_requests: DEFAULT_REQUESTS.length
			}),
			operatorTurns: DEFAULT_OPERATOR_TURNS
		}),
		sessionStore: buildMockSessionListStore({
			sessions: [buildSummary({ title: 'Reorganize settings nav and routes' })]
		}),
		flowStore: scaleRecoveryFlowStore
	}}
	loaders={[
		async () => {
			scaleRecoveryRecovered.current = false;
			scaleRecoveryFlowStore.deactivate();
			stubSessionOverviewFetch({
				sessionId: STORY_SESSION_ID,
				requests: DEFAULT_REQUESTS,
				kickoffRequestId: DEFAULT_REQUESTS[0]!.id,
				kickoffUserPrompt: KICKOFF_PROMPT,
				summary: buildSummary({ request_count: DEFAULT_REQUESTS.length }),
				toolUses: buildSettingsToolUses(STORY_SESSION_ID),
				operatorTurns: DEFAULT_OPERATOR_TURNS
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}?view=flow`),
					params: { sessionId: STORY_SESSION_ID }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('Session flow')).toBeInTheDocument());

		await waitFor(() =>
			expect(canvas.getAllByRole('button', { name: /^Inspect Worker \d+/ })).toHaveLength(
				SCALE_WORKER_COUNT
			)
		);
		await expect(canvas.queryByRole('alert')).not.toBeInTheDocument();

		const stream = new ProxyEventStream();
		const adapter = createSessionFlowLiveAdapter({
			flowStore: scaleRecoveryFlowStore,
			resolveRequestSessionId: () => STORY_SESSION_ID
		});
		stream.state = 'connected';
		adapter.observeConnectionState(stream.state);
		stream.state = 'reconnecting';
		adapter.observeConnectionState(stream.state);
		await expect(canvas.getByText('STALE')).not.toBeVisible();

		scaleRecoveryRecovered.current = true;
		stream.state = 'connected';
		adapter.observeConnectionState(stream.state);

		await waitFor(() => expect(canvas.getByText('STALE')).toBeVisible());
		await waitFor(() =>
			expect(canvas.getAllByRole('button', { name: /^Inspect Worker \d+/ })).toHaveLength(
				SCALE_WORKER_COUNT + 1
			)
		);
		await waitFor(() => expect(canvas.getByText('STALE')).not.toBeVisible());
		await expect(canvas.queryByRole('alert')).not.toBeInTheDocument();

		adapter.dispose();
		stream.disconnect();
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionDetailRouteHarness {...args} />
		</div>
	{/snippet}
</Story>
