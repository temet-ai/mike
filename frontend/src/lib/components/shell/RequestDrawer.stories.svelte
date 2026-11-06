<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import RequestDrawerHarness from './__harness__/RequestDrawerHarness.svelte';
	import type { RequestDetail } from '$lib/types';
	import { buildRequestDetail } from '$lib/components/__fixtures__/request';
	import { stubBandBodyFetch } from '$lib/components/__fixtures__/bandBodies';
	import { buildMockDetailStore } from '$lib/test-support';
	import { openDrawer, closeDrawer, shellState } from '$lib/stores/shellState.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const STORY_SESSION_ID = 'storybook-session-1';
	const STORY_BAND_ID = 101;

	const SSE_DETAIL = buildRequestDetail({
		id: STORY_BAND_ID,
		is_streaming: true,
		accumulated_text:
			'The authentication system works by validating the JWT token against the public key stored in the environment variables. Here is how to implement it in your codebase.',
		accumulated_thinking:
			'The user is asking about JWT authentication. I should explain the key components clearly.',
		tool_calls: [
			{
				tool_id: 'toolu_01Abc',
				name: 'Read',
				input_json: '{"file_path": "/src/auth/jwt.ts"}',
				complete: true
			}
		],
		diagnostics: []
	});

	const FINDINGS_DETAIL = buildRequestDetail({
		id: STORY_BAND_ID,
		diagnostics: [
			{
				analyzer: 'weasel',
				category: 'justifying_shortcuts',
				matched_text: 'I think',
				advice: 'Be more direct.',
				match_range: [0, 7],
				other_hits: 0,
				created_at: '2026-05-04T10:00:00Z'
			},
			{
				analyzer: 'weasel',
				category: 'deflecting_blame',
				matched_text: 'I apologize',
				advice: 'Avoid unnecessary apology.',
				match_range: [0, 11],
				other_hits: 0,
				created_at: '2026-05-04T10:00:00Z'
			}
		]
	});

	const EMPTY_EVENTS_DETAIL = buildRequestDetail({
		id: STORY_BAND_ID,
		is_streaming: false,
		accumulated_text: '',
		accumulated_thinking: '',
		tool_calls: [],
		diagnostics: []
	});

	const INCOMPLETE_TOOL_DETAIL = buildRequestDetail({
		id: STORY_BAND_ID,
		is_streaming: true,
		accumulated_text: '',
		accumulated_thinking: '',
		tool_calls: [
			{
				tool_id: 'toolu_partial01',
				name: 'Edit',
				input_json: '{"file_path": "/src/auth/jwt.ts", "old_string": "const token = jwt.sign(payl',
				complete: false
			}
		],
		diagnostics: []
	});

	const THINKING_ONLY_DETAIL = buildRequestDetail({
		id: STORY_BAND_ID,
		is_streaming: true,
		accumulated_text: '',
		accumulated_thinking:
			'The user wants the JWT refresh flow. I need to: (1) read the existing token store, (2) add an expiry check, (3) wire a refresh endpoint. Let me start by inspecting the current implementation before making any edits.',
		tool_calls: [],
		diagnostics: []
	});

	const SSE_MAP = new Map([[STORY_BAND_ID, SSE_DETAIL]]);
	const FINDINGS_MAP = new Map([[STORY_BAND_ID, FINDINGS_DETAIL]]);
	const EMPTY_EVENTS_MAP = new Map([[STORY_BAND_ID, EMPTY_EVENTS_DETAIL]]);
	const INCOMPLETE_TOOL_MAP = new Map([[STORY_BAND_ID, INCOMPLETE_TOOL_DETAIL]]);
	const THINKING_ONLY_MAP = new Map([[STORY_BAND_ID, THINKING_ONLY_DETAIL]]);

	const BODY_DETAIL = buildRequestDetail({
		id: STORY_BAND_ID,
		is_streaming: false,
		diagnostics: []
	});
	const BODY_MAP = new Map([[STORY_BAND_ID, BODY_DETAIL]]);

	const CLOSED_STORE = buildMockDetailStore();
	const OPEN_REQUEST_STORE = buildMockDetailStore({ detailById: BODY_MAP });
	const OPEN_RESPONSE_STORE = buildMockDetailStore({ detailById: BODY_MAP });
	const OPEN_EVENTS_STORE = buildMockDetailStore({ detailById: SSE_MAP });
	const OPEN_FINDINGS_STORE = buildMockDetailStore({ detailById: FINDINGS_MAP });
	const DETAIL_LOADING_STORE = buildMockDetailStore();
	const EMPTY_EVENTS_STORE = buildMockDetailStore({ detailById: EMPTY_EVENTS_MAP });
	const INCOMPLETE_TOOL_STORE = buildMockDetailStore({ detailById: INCOMPLETE_TOOL_MAP });
	const THINKING_ONLY_STORE = buildMockDetailStore({ detailById: THINKING_ONLY_MAP });

	const { Story } = defineMeta({
		title: 'Shell/RequestDrawer',
		component: RequestDrawerHarness,
		tags: ['autodocs'],
		parameters: { a11y: { test: 'todo' } },
		argTypes: {
			detailStore: { control: false }
		}
	});
</script>

<Story
	name="Closed"
	args={{ detailStore: CLOSED_STORE }}
	loaders={[
		async () => {
			closeDrawer();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_BAND_ID}`),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_BAND_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="relative h-[400px] w-[600px] bg-[var(--surface-bg)]">
			<p class="p-4 text-[11px] text-[var(--text-3)]">Drawer is closed — no visible output.</p>
			<RequestDrawerHarness detailStore={args.detailStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="OpenRequest"
	args={{ detailStore: OPEN_REQUEST_STORE }}
	loaders={[
		async () => {
			stubBandBodyFetch();
			openDrawer(STORY_BAND_ID, 'request');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_BAND_ID}?drawer=request`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_BAND_ID) }
				}
			}
		}
	}}
	play={async () => {
		const body = within(document.body);
		await expect(await body.findByRole('dialog')).toBeInTheDocument();

		const sentTab = await body.findByRole('tab', { name: 'Sent' });
		await expect(sentTab).toHaveAttribute('aria-selected', 'false');
		await userEvent.click(sentTab);
		await expect(sentTab).toHaveAttribute('aria-selected', 'true');

		const closeBtn = body.getByRole('button', { name: /close/i });
		await userEvent.click(closeBtn);
		await expect(shellState.drawerKind).toBeNull();
	}}
>
	{#snippet template(args)}
		<div class="relative h-[600px] w-[600px] bg-[var(--surface-bg)]">
			<RequestDrawerHarness detailStore={args.detailStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="OpenResponse"
	args={{ detailStore: OPEN_RESPONSE_STORE }}
	loaders={[
		async () => {
			stubBandBodyFetch();
			openDrawer(STORY_BAND_ID, 'response');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_BAND_ID}?drawer=response`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_BAND_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="relative h-[600px] w-[600px] bg-[var(--surface-bg)]">
			<RequestDrawerHarness detailStore={args.detailStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="OpenEvents"
	args={{ detailStore: OPEN_EVENTS_STORE }}
	loaders={[
		async () => {
			openDrawer(STORY_BAND_ID, 'events');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_BAND_ID}?drawer=events`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_BAND_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="relative h-[700px] w-[600px] bg-[var(--surface-bg)]">
			<RequestDrawerHarness detailStore={args.detailStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="OpenFindings"
	args={{ detailStore: OPEN_FINDINGS_STORE }}
	loaders={[
		async () => {
			openDrawer(STORY_BAND_ID, 'findings');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_BAND_ID}?drawer=findings`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_BAND_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="relative h-[600px] w-[600px] bg-[var(--surface-bg)]">
			<RequestDrawerHarness detailStore={args.detailStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="DetailLoading"
	args={{ detailStore: DETAIL_LOADING_STORE }}
	loaders={[
		async () => {
			openDrawer(STORY_BAND_ID, 'events');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_BAND_ID}?drawer=events`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_BAND_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="relative h-[600px] w-[600px] bg-[var(--surface-bg)]">
			<RequestDrawerHarness detailStore={args.detailStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="EmptyEvents"
	args={{ detailStore: EMPTY_EVENTS_STORE }}
	loaders={[
		async () => {
			openDrawer(STORY_BAND_ID, 'events');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_BAND_ID}?drawer=events`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_BAND_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="relative h-[600px] w-[600px] bg-[var(--surface-bg)]">
			<RequestDrawerHarness detailStore={args.detailStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="IncompleteToolCall"
	args={{ detailStore: INCOMPLETE_TOOL_STORE }}
	loaders={[
		async () => {
			openDrawer(STORY_BAND_ID, 'events');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_BAND_ID}?drawer=events`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_BAND_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="relative h-[600px] w-[600px] bg-[var(--surface-bg)]">
			<RequestDrawerHarness detailStore={args.detailStore} />
		</div>
	{/snippet}
</Story>

<Story
	name="ThinkingOnly"
	args={{ detailStore: THINKING_ONLY_STORE }}
	loaders={[
		async () => {
			openDrawer(STORY_BAND_ID, 'events');
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(
						`http://localhost/sessions/${STORY_SESSION_ID}/requests/${STORY_BAND_ID}?drawer=events`
					),
					params: { sessionId: STORY_SESSION_ID, requestId: String(STORY_BAND_ID) }
				}
			}
		}
	}}
>
	{#snippet template(args)}
		<div class="relative h-[600px] w-[600px] bg-[var(--surface-bg)]">
			<RequestDrawerHarness detailStore={args.detailStore} />
		</div>
	{/snippet}
</Story>
