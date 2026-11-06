<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SessionRequestRouteHarness from './__harness__/SessionRequestRouteHarness.svelte';
	import type { RequestSummary, SessionDetail, SessionSummary } from '$lib/types';
	import {
		buildMockDetailStore,
		buildMockSessionListStore,
		stubFetch404,
		stubFetchErrorFor
	} from '$lib/test-support';

	const STORY_SESSION_ID = 'storybook-session-1';
	const BASE_TIME = '2026-04-17T12:';

	function buildSummary(overrides: Partial<SessionSummary> = {}): SessionSummary {
		return {
			id: STORY_SESSION_ID,
			project: '/Users/dev/project',
			project_id: 1,
			cwd: '/Users/dev/project',
			project_resolution: 'git',
			first_seen: '2026-04-17T11:00:00Z',
			last_seen: '2026-04-17T12:04:30Z',
			request_count: 3,
			models: ['claude-opus-4-7'],
			last_status: 'Complete',
			title: null,
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
		turn_origin?: RequestSummary['turn_origin'];
		subagent_persona?: string | null;
	}): RequestSummary {
		return {
			id: overrides.id,
			method: 'POST',
			path: '/v1/messages',
			model: overrides.model ?? 'claude-opus-4-7',
			status: overrides.status ?? 'Complete',
			is_streaming: overrides.status === 'Streaming',
			message_count: 4,
			max_tokens: null,
			created_at: overrides.created_at ?? '2026-04-17T12:00:00Z',
			completed_at:
				overrides.completed_at !== undefined ? overrides.completed_at : '2026-04-17T12:00:05Z',
			session_id: STORY_SESSION_ID,
			project: '/Users/dev/project',
			human_prompt:
				'Write a comprehensive test suite for the authentication module including unit tests for the JWT validation logic and integration tests for the OAuth2 flow.',
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
			id: 101,
			status: 'Complete',
			created_at: `${BASE_TIME}00:00Z`,
			completed_at: `${BASE_TIME}00:04Z`
		}),
		buildRequest({
			id: 102,
			status: 'Complete',
			model: 'claude-sonnet-4-6',
			created_at: `${BASE_TIME}01:30Z`,
			completed_at: `${BASE_TIME}01:35Z`
		}),
		buildRequest({
			id: 103,
			status: 'Streaming',
			created_at: `${BASE_TIME}02:00Z`,
			completed_at: null
		})
	];

	const DEFAULT_DETAIL = buildDetail(DEFAULT_REQUESTS);
	const PINNED_DETAIL = buildDetail(DEFAULT_REQUESTS, {
		summary: buildSummary({
			pinned: true,
			title: 'Auth module test suite',
			request_count: DEFAULT_REQUESTS.length
		})
	});

	const { Story } = defineMeta({
		title: 'Routes/Sessions/Request',
		component: SessionRequestRouteHarness,
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
		detailStore: buildMockDetailStore({ detail: DEFAULT_DETAIL }),
		sessionStore: buildMockSessionListStore({ sessions: [buildSummary()] })
	}}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}/requests/42`),
					params: { sessionId: STORY_SESSION_ID, requestId: '42' }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionRequestRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="NoSelection"
	args={{
		detailStore: buildMockDetailStore({ detail: DEFAULT_DETAIL }),
		sessionStore: buildMockSessionListStore({ sessions: [buildSummary()] })
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
			<SessionRequestRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="WithSessionTitle"
	args={{
		detailStore: buildMockDetailStore({ detail: PINNED_DETAIL }),
		sessionStore: buildMockSessionListStore({
			sessions: [buildSummary({ pinned: true, title: 'Auth module test suite' })]
		})
	}}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}/requests/101`),
					params: { sessionId: STORY_SESSION_ID, requestId: '101' }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionRequestRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="RequestNotFound"
	args={{
		detailStore: buildMockDetailStore({ detail: DEFAULT_DETAIL }),
		sessionStore: buildMockSessionListStore({ sessions: [buildSummary()] })
	}}
	loaders={[
		async () => {
			stubFetch404({ error: 'request not found' });
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}/requests/99999`),
					params: { sessionId: STORY_SESSION_ID, requestId: '99999' }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionRequestRouteHarness {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="RequestLoadError"
	args={{
		detailStore: buildMockDetailStore({ detail: DEFAULT_DETAIL }),
		sessionStore: buildMockSessionListStore({ sessions: [buildSummary()] })
	}}
	loaders={[
		async () => {
			stubFetchErrorFor('/api/requests/', 'request store unavailable');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}/requests/101`),
					params: { sessionId: STORY_SESSION_ID, requestId: '101' }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[900px] h-[700px] bg-[var(--surface-bg)] overflow-hidden">
			<SessionRequestRouteHarness {...args} />
		</div>
	{/snippet}
</Story>
