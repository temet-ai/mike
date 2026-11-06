<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RequestDetailColumnHarness from './__harness__/RequestDetailColumnHarness.svelte';
	import {
		buildSessionSummary,
		buildSessionDetail,
		STORY_SESSION_ID
	} from '$lib/components/__fixtures__/session';
	import { buildRequestSummary } from '$lib/components/__fixtures__/request';
	import { stubBandBodyFetch } from '$lib/components/__fixtures__/bandBodies';
	import { buildMockDetailStore, buildMockSessionListStore } from '$lib/test-support';
	import { __resetDetailCacheForTests } from '$lib/stores/cache/requestDetailStore.svelte';
	import { within, expect, waitFor } from 'storybook/test';
	import { __setRetryDelaysForTesting, __resetRetryDelaysForTesting } from '$lib/api';

	const STORY_REQUEST_ID = 101;

	const DEFAULT_REQUESTS = [
		buildRequestSummary({
			id: 100,
			session_id: STORY_SESSION_ID,
			message_count: 6,
			created_at: '2026-04-17T12:00:00Z',
			completed_at: '2026-04-17T12:00:03Z'
		}),
		buildRequestSummary({
			id: 101,
			session_id: STORY_SESSION_ID,
			message_count: 12,
			created_at: '2026-04-17T12:01:00Z',
			completed_at: '2026-04-17T12:01:03Z'
		}),
		buildRequestSummary({
			id: 102,
			session_id: STORY_SESSION_ID,
			message_count: 18,
			created_at: '2026-04-17T12:02:00Z',
			completed_at: '2026-04-17T12:02:03Z'
		})
	];

	const DEFAULT_DETAIL = buildSessionDetail({ requests: DEFAULT_REQUESTS });

	const NO_REQUEST_DETAIL = buildMockDetailStore({
		detail: DEFAULT_DETAIL,
		currentId: STORY_SESSION_ID
	});
	const NO_REQUEST_SESSION = buildMockSessionListStore({
		sessions: [buildSessionSummary({ id: STORY_SESSION_ID, request_count: 3 })]
	});
	const WITH_REQUEST_DETAIL = buildMockDetailStore({
		detail: DEFAULT_DETAIL,
		currentId: STORY_SESSION_ID
	});
	const WITH_REQUEST_SESSION = buildMockSessionListStore({
		sessions: [buildSessionSummary({ id: STORY_SESSION_ID, request_count: 3 })]
	});
	const WITH_TITLE_DETAIL = buildMockDetailStore({
		detail: DEFAULT_DETAIL,
		currentId: STORY_SESSION_ID
	});
	const WITH_TITLE_SESSION = buildMockSessionListStore({
		sessions: [
			buildSessionSummary({
				id: STORY_SESSION_ID,
				title: 'Debugging the auth middleware',
				pinned: false
			})
		]
	});
	const PINNED_DETAIL = buildMockDetailStore({
		detail: DEFAULT_DETAIL,
		currentId: STORY_SESSION_ID
	});
	const PINNED_SESSION = buildMockSessionListStore({
		sessions: [buildSessionSummary({ id: STORY_SESSION_ID, title: 'Pinned session', pinned: true })]
	});

	const THIN_CONTEXT_DETAIL = buildMockDetailStore({
		detail: DEFAULT_DETAIL,
		currentId: STORY_SESSION_ID
	});
	const THIN_CONTEXT_SESSION = buildMockSessionListStore({
		sessions: [
			buildSessionSummary({
				id: STORY_SESSION_ID,
				request_count: 7,
				first_seen: '',
				models: ['claude-opus-4-7']
			})
		]
	});

	const NO_SUMMARY_DETAIL = buildMockDetailStore({
		detail: DEFAULT_DETAIL,
		currentId: STORY_SESSION_ID
	});
	const NO_SUMMARY_SESSION = buildMockSessionListStore({
		sessions: [buildSessionSummary({ id: 'a-different-session', request_count: 2 })]
	});

	const { Story } = defineMeta({
		title: 'Shell/RequestDetailColumn',
		component: RequestDetailColumnHarness,
		tags: ['autodocs'],
		argTypes: {
			detailStore: { control: false },
			sessionStore: { control: false }
		}
	});
</script>

<Story
	name="NoRequest"
	args={{ detailStore: NO_REQUEST_DETAIL, sessionStore: NO_REQUEST_SESSION }}
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
		<div style="width: 700px; height: 600px; background: var(--surface-bg); overflow: hidden;">
			<RequestDetailColumnHarness detailStore={args.detailStore} sessionStore={args.sessionStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="WithRequest"
	args={{ detailStore: WITH_REQUEST_DETAIL, sessionStore: WITH_REQUEST_SESSION }}
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
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_REQUEST_ID}`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_REQUEST_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div style="width: 700px; height: 600px; background: var(--surface-bg); overflow: hidden;">
			<RequestDetailColumnHarness detailStore={args.detailStore} sessionStore={args.sessionStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="ShowErr"
	args={{ detailStore: WITH_REQUEST_DETAIL, sessionStore: WITH_REQUEST_SESSION }}
	loaders={[
		async () => {
			__resetDetailCacheForTests();
			__setRetryDelaysForTesting([0, 0, 0]);
			globalThis.fetch = async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (/\/api\/requests\/\d+(?:[?#]|$)/.test(url)) {
					return new Response(JSON.stringify({ error: 'request not found' }), {
						status: 500,
						statusText: 'Internal Server Error',
						headers: { 'content-type': 'application/json' }
					});
				}
				return new Response('Not Found', { status: 404 });
			};
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}/requests/90901`),
					params: { sessionId: STORY_SESSION_ID, requestId: '90901' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(
			() => {
				const el = canvasElement.querySelector('[data-slot="load-error-banner"]');
				if (!el) throw new Error('load-error-banner not yet rendered');
				expect(el).toHaveTextContent(/Couldn't load this request/i);
			},
			{ timeout: 5000 }
		);
		__resetRetryDelaysForTesting();
		void canvas;
	}}
>
	{#snippet template(args)}
		<div style="width: 700px; height: 600px; background: var(--surface-bg); overflow: hidden;">
			<RequestDetailColumnHarness detailStore={args.detailStore} sessionStore={args.sessionStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="WithSessionTitle"
	args={{ detailStore: WITH_TITLE_DETAIL, sessionStore: WITH_TITLE_SESSION }}
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
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_REQUEST_ID}`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_REQUEST_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div style="width: 700px; height: 600px; background: var(--surface-bg); overflow: hidden;">
			<RequestDetailColumnHarness detailStore={args.detailStore} sessionStore={args.sessionStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="Pinned"
	args={{ detailStore: PINNED_DETAIL, sessionStore: PINNED_SESSION }}
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
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_REQUEST_ID}`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_REQUEST_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div style="width: 700px; height: 600px; background: var(--surface-bg); overflow: hidden;">
			<RequestDetailColumnHarness detailStore={args.detailStore} sessionStore={args.sessionStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="ThinContext"
	args={{ detailStore: THIN_CONTEXT_DETAIL, sessionStore: THIN_CONTEXT_SESSION }}
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
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_REQUEST_ID}`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_REQUEST_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div style="width: 700px; height: 600px; background: var(--surface-bg); overflow: hidden;">
			<RequestDetailColumnHarness detailStore={args.detailStore} sessionStore={args.sessionStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="NoSummary"
	args={{ detailStore: NO_SUMMARY_DETAIL, sessionStore: NO_SUMMARY_SESSION }}
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
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_REQUEST_ID}`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_REQUEST_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div style="width: 700px; height: 600px; background: var(--surface-bg); overflow: hidden;">
			<RequestDetailColumnHarness detailStore={args.detailStore} sessionStore={args.sessionStore} />
		</div>
	{/snippet}
</Story>
