<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SessionsColumnHarness from './__harness__/SessionsColumnHarness.svelte';
	import type { SessionSummary } from '$lib/types';
	import {
		buildSessionSummary,
		STORY_SESSION_ID,
		STORY_PROJECT
	} from '$lib/components/__fixtures__/session';
	import { buildMockSessionListStore } from '$lib/test-support/storyMocks';
	const STATIC_NOW = new Date('2026-04-17T12:10:00Z').getTime();

	const DEFAULT_SESSIONS: SessionSummary[] = [
		buildSessionSummary({
			id: STORY_SESSION_ID,
			last_seen: '2026-04-17T12:05:00Z',
			request_count: 5
		}),
		buildSessionSummary({
			id: 'session-2',
			last_seen: '2026-04-17T11:30:00Z',
			request_count: 2,
			last_status: 'Streaming'
		}),
		buildSessionSummary({
			id: 'session-3',
			last_seen: '2026-04-17T10:00:00Z',
			request_count: 12,
			last_status: 'Complete',
			title: 'Auth module tests'
		})
	];

	const MULTI_PROJECT_SESSIONS: SessionSummary[] = [
		buildSessionSummary({
			id: 'sess-a1',
			project: '/Users/dev/project-a',
			last_seen: '2026-04-17T12:05:00Z',
			request_count: 3
		}),
		buildSessionSummary({
			id: 'sess-a2',
			project: '/Users/dev/project-a',
			last_seen: '2026-04-17T11:00:00Z',
			request_count: 1
		}),
		buildSessionSummary({
			id: 'sess-b1',
			project: '/Users/dev/long-project-name-that-wraps',
			last_seen: '2026-04-17T10:30:00Z',
			request_count: 7
		}),
		buildSessionSummary({
			id: 'sess-c1',
			project: null,
			last_seen: '2026-04-17T09:00:00Z',
			request_count: 1
		})
	];

	const PINNED_AND_MANY_SESSIONS: SessionSummary[] = [
		buildSessionSummary({
			id: 'pinned-1',
			pinned: true,
			title: 'Important debug session',
			last_seen: '2026-04-17T12:00:00Z',
			request_count: 20
		}),
		...Array.from({ length: 7 }, (_, i) =>
			buildSessionSummary({
				id: `unpinned-${i}`,
				last_seen: `2026-04-17T1${i}:00:00Z`,
				request_count: i + 1,
				title: i % 2 === 0 ? `Session with a title ${i}` : null
			})
		)
	];

	const UNGROUPED_SESSIONS: SessionSummary[] = [
		buildSessionSummary({
			id: 'ung-1',
			project: null,
			last_seen: '2026-04-17T12:00:00Z',
			request_count: 3
		}),
		buildSessionSummary({
			id: 'ung-2',
			project: null,
			last_seen: '2026-04-17T11:00:00Z',
			request_count: 1
		})
	];

	const LIVE_SESSIONS: SessionSummary[] = [
		buildSessionSummary({
			id: 'live-streaming',
			last_seen: '2026-04-17T12:09:50Z',
			request_count: 4,
			last_status: 'Streaming',
			title: 'Streaming response in flight'
		}),
		buildSessionSummary({
			id: 'live-pending',
			last_seen: '2026-04-17T12:09:40Z',
			request_count: 1,
			last_status: 'Pending'
		})
	];

	const OVERFLOW_SESSIONS: SessionSummary[] = Array.from({ length: 8 }, (_, i) =>
		buildSessionSummary({
			id: `overflow-${i}`,
			last_seen: `2026-04-17T1${i}:00:00Z`,
			request_count: i + 1,
			title: `Session ${i}`
		})
	);

	const DEFAULT_STORE = buildMockSessionListStore({ sessions: DEFAULT_SESSIONS });
	const LOAD_MORE_ERROR_STORE = buildMockSessionListStore({
		sessions: DEFAULT_SESSIONS,
		hasMore: true,
		loadMoreError:
			'Failed to load older sessions: the inspect API returned 503 (database connection pool exhausted). The already-loaded sessions above are unaffected; retry once capacity is restored.'
	});
	const LIVE_STORE = buildMockSessionListStore({ sessions: LIVE_SESSIONS });
	const OVERFLOW_STORE = buildMockSessionListStore({ sessions: OVERFLOW_SESSIONS });
	const COLLAPSED_STORE = buildMockSessionListStore({ sessions: DEFAULT_SESSIONS });
	const EMPTY_STORE = buildMockSessionListStore({ sessions: [] });
	const LOADING_STORE = buildMockSessionListStore({ sessions: [], loading: true });
	const MULTI_PROJECT_STORE = buildMockSessionListStore({ sessions: MULTI_PROJECT_SESSIONS });
	const PINNED_STORE = buildMockSessionListStore({ sessions: PINNED_AND_MANY_SESSIONS });
	const HAS_MORE_STORE = buildMockSessionListStore({ sessions: DEFAULT_SESSIONS, hasMore: true });
	const UNGROUPED_STORE = buildMockSessionListStore({ sessions: UNGROUPED_SESSIONS });

	const { Story } = defineMeta({
		title: 'Draft/SessionsColumn',
		component: SessionsColumnHarness,
		tags: ['autodocs'],
		argTypes: {
			sessionStore: { control: false }
		}
	});
</script>

<Story
	name="Default"
	args={{ sessionStore: DEFAULT_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`)
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>

<Story
	name="Empty"
	args={{ sessionStore: EMPTY_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL('http://localhost/') }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>

<Story
	name="Loading"
	args={{ sessionStore: LOADING_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL('http://localhost/') }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>

<Story
	name="MultiProject"
	args={{ sessionStore: MULTI_PROJECT_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL('http://localhost/sessions/sess-a1') }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>

<Story
	name="WithPinnedAndMany"
	args={{ sessionStore: PINNED_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL('http://localhost/sessions/pinned-1') }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>

<Story
	name="HasMore"
	args={{ sessionStore: HAS_MORE_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`) }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>

<Story
	name="Ungrouped"
	args={{ sessionStore: UNGROUPED_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL('http://localhost/sessions/ung-1') }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>

<Story
	name="LoadMoreError"
	args={{ sessionStore: LOAD_MORE_ERROR_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`) }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>

<Story
	name="CollapsedGroup"
	args={{ sessionStore: COLLAPSED_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}`) }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness
				sessionStore={args.sessionStore}
				now={args.now}
				collapsedProject={STORY_PROJECT}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="LiveSessions"
	args={{ sessionStore: LIVE_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL('http://localhost/sessions/live-streaming') }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>

<Story
	name="UnpinnedOverflow"
	args={{ sessionStore: OVERFLOW_STORE, now: STATIC_NOW }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: { url: new URL('http://localhost/sessions/overflow-0') }
			}
		}
	}}
>
	{#snippet template(args)}
		<div
			style="width: 260px; height: 600px; background: var(--surface-bg); overflow: hidden; position: relative;"
		>
			<SessionsColumnHarness sessionStore={args.sessionStore} now={args.now} />
		</div>
	{/snippet}
</Story>
