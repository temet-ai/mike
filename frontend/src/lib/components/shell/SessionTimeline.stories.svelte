<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, waitFor, userEvent } from 'storybook/test';
	import SessionTimelineHarness from './__harness__/SessionTimelineHarness.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import type {
		OperatorTurnDto,
		RequestSummary,
		SessionDetail,
		ToolUseDto,
		ToolUsesResponse
	} from '$lib/types';
	import { buildRequestSummary } from '$lib/components/__fixtures__/request';
	import { buildSessionSummary, STORY_SESSION_ID } from '$lib/components/__fixtures__/session';
	import { buildMockDetailStore } from '$lib/test-support';
	import type { MailboxMessage } from '$lib/api/mailbox';

	const BASE_TIME = '2026-07-02T14:';
	const STATIC_NOW = new Date('2026-07-02T14:20:00Z').getTime();

	function buildOperatorTurn(overrides: Partial<OperatorTurnDto> = {}): OperatorTurnDto {
		return {
			turn_id: 1,
			session_id: STORY_SESSION_ID,
			project_id: 1,
			start_request_id: 101,
			end_request_id: 103,
			start_request_number: 2,
			end_request_number: 4,
			user_prompt: 'Placeholder prompt.',
			final_assistant_text: 'Placeholder reply.',
			tool_call_count: 0,
			tool_error_count: 0,
			files_touched: [],
			tools_used: [],
			agents_dispatched: 0,
			weasel_findings_count: 0,
			total_input_tokens: 0,
			total_output_tokens: 0,
			duration_ms: 0,
			outcome: 'unknown',
			outcome_evidence: null,
			classified_at: null,
			created_at: `${BASE_TIME}00:00Z`,
			updated_at: `${BASE_TIME}00:00Z`,
			stop_response: null,
			stop_reason: 'end_turn',
			stop_request_id: null,
			interrupted: false,
			...overrides
		};
	}

	function buildToolUse(overrides: Partial<ToolUseDto> = {}): ToolUseDto {
		return {
			request_id: 102,
			session_id: STORY_SESSION_ID,
			created_at: `${BASE_TIME}02:00Z`,
			tool_id: 'toolu_default',
			tool_name: 'Read',
			input: { file_path: 'src/lib/foo.ts' },
			file_path: 'src/lib/foo.ts',
			ord: 0,
			result: {
				tool_use_id: 'toolu_default',
				is_error: false,
				content: '1 → export const foo = 1;',
				content_sha256: 'sha',
				content_bytes: 200
			},
			...overrides
		};
	}

	const TURN_1 = buildOperatorTurn({
		turn_id: 1,
		start_request_id: 101,
		end_request_id: 103,
		start_request_number: 2,
		end_request_number: 4,
		user_prompt:
			'Do a pass over the session timeline so a session and its tools read coherently. I want to find a specific user prompt, see what Claude replied, and tell which tools ran — fast.',
		final_assistant_text:
			'Mapped the surface. Flattened each turn into prompt → readable tool cards → reply, defaulting turns to collapsed so a thousand-request session stays a short scannable list.',
		stop_response:
			'Mapped the surface. Flattened each turn into prompt → readable tool cards → reply, defaulting turns to collapsed so a thousand-request session stays a short scannable list.',
		tool_call_count: 2,
		tool_error_count: 0,
		files_touched: ['src/lib/foo.ts'],
		tools_used: ['Read', 'Edit'],
		total_input_tokens: 48200,
		total_output_tokens: 3100,
		duration_ms: 5200,
		outcome: 'accepted',
		created_at: `${BASE_TIME}02:00Z`
	});
	const TURN_2 = buildOperatorTurn({
		turn_id: 2,
		start_request_id: 105,
		end_request_id: 109,
		start_request_number: 6,
		end_request_number: 10,
		user_prompt:
			'The whole thing is too fat and does not fit the style. How does it hold up at a thousand requests?',
		final_assistant_text:
			'Rebuilt around dense, readable tool cards. First cut failed the test suite; fixed. Also dispatched a sub-agent to double-check the card density at scale.',
		stop_response:
			'Rebuilt around dense, readable tool cards. First cut failed the test suite; fixed. Also dispatched a sub-agent to double-check the card density at scale.',
		tool_call_count: 2,
		tool_error_count: 1,
		files_touched: [],
		tools_used: ['Bash', 'Task'],
		agents_dispatched: 1,
		total_input_tokens: 61400,
		total_output_tokens: 5200,
		duration_ms: 9800,
		outcome: 'corrected',
		created_at: `${BASE_TIME}09:00Z`
	});
	const TURN_3 = buildOperatorTurn({
		turn_id: 3,
		start_request_id: 111,
		end_request_id: 112,
		start_request_number: 12,
		end_request_number: 13,
		user_prompt: 'Now add the housekeeping fold treatment and ship it.',
		final_assistant_text: '',
		stop_response: null,
		tool_call_count: 0,
		tool_error_count: 0,
		outcome: 'unknown',
		created_at: `${BASE_TIME}20:00Z`
	});
	const FIXTURE_TURNS = [TURN_1, TURN_2, TURN_3];

	function band(overrides: Partial<RequestSummary>): RequestSummary {
		return buildRequestSummary({ session_id: STORY_SESSION_ID, ...overrides });
	}
	const FIXTURE_REQUESTS: RequestSummary[] = [
		band({
			id: 100,
			request_number: 1,
			turn_origin: 'quota_probe',
			message_count: 0,
			created_at: `${BASE_TIME}01:59Z`,
			tool_names: []
		}),
		band({
			id: 101,
			request_number: 2,
			turn_origin: 'primary_first',
			created_at: `${BASE_TIME}02:00Z`
		}),
		band({
			id: 102,
			request_number: 3,
			turn_origin: 'primary',
			turn_type: 'tool_loop',
			created_at: `${BASE_TIME}02:04Z`,
			tool_names: ['Read']
		}),
		band({
			id: 103,
			request_number: 4,
			turn_origin: 'primary',
			turn_type: 'tool_loop',
			created_at: `${BASE_TIME}02:08Z`,
			tool_names: ['Edit']
		}),
		band({
			id: 104,
			request_number: 5,
			turn_origin: 'title_sidecar',
			message_count: 0,
			created_at: `${BASE_TIME}08:55Z`,
			tool_names: []
		}),
		band({ id: 105, request_number: 6, turn_origin: 'primary', created_at: `${BASE_TIME}09:00Z` }),
		band({
			id: 106,
			request_number: 7,
			turn_origin: 'primary',
			turn_type: 'tool_loop',
			created_at: `${BASE_TIME}09:04Z`,
			tool_names: ['Bash'],
			status: 'Complete'
		}),
		band({
			id: 107,
			request_number: 8,
			turn_origin: 'primary',
			turn_type: 'tool_loop',
			created_at: `${BASE_TIME}09:08Z`,
			tool_names: ['Bash']
		}),
		band({
			id: 108,
			request_number: 9,
			turn_origin: 'subagent',
			subagent_persona: 'Test Runner',
			turn_type: 'tool_loop',
			created_at: `${BASE_TIME}09:12Z`,
			tool_names: ['Task']
		}),
		band({
			id: 109,
			request_number: 10,
			turn_origin: 'primary',
			turn_type: 'continuation',
			created_at: `${BASE_TIME}09:18Z`
		}),
		band({
			id: 110,
			request_number: 11,
			turn_origin: 'monitor_sidecar',
			message_count: 0,
			created_at: `${BASE_TIME}19:58Z`,
			tool_names: []
		}),
		band({
			id: 111,
			request_number: 12,
			turn_origin: 'primary',
			status: 'Streaming',
			is_streaming: true,
			completed_at: null,
			created_at: `${BASE_TIME}20:00Z`
		}),
		band({
			id: 112,
			request_number: 13,
			turn_origin: 'primary',
			turn_type: 'tool_loop',
			status: 'Streaming',
			is_streaming: true,
			completed_at: null,
			created_at: `${BASE_TIME}20:04Z`
		})
	];

	const FIXTURE_TOOL_USES: ToolUseDto[] = [
		buildToolUse({
			tool_id: 'toolu_1',
			request_id: 102,
			tool_name: 'Read',
			input: { file_path: 'shell/SessionTimeline.svelte' },
			file_path: 'shell/SessionTimeline.svelte',
			ord: 0,
			result: {
				tool_use_id: 'toolu_1',
				is_error: false,
				content: '1363 lines — band derivation + scroll wiring',
				content_sha256: 's',
				content_bytes: 38000
			}
		}),
		buildToolUse({
			tool_id: 'toolu_2',
			request_id: 103,
			tool_name: 'Edit',
			input: {
				file_path: 'shell/TurnCard.svelte',
				old_string: 'collapsed',
				new_string: 'expanded'
			},
			file_path: 'shell/TurnCard.svelte',
			ord: 0,
			result: {
				tool_use_id: 'toolu_2',
				is_error: false,
				content: 'Applied 1 edit',
				content_sha256: 's',
				content_bytes: 400
			}
		}),
		buildToolUse({
			tool_id: 'toolu_3',
			request_id: 106,
			tool_name: 'Bash',
			input: { command: 'pnpm run check' },
			file_path: null,
			ord: 0,
			result: {
				tool_use_id: 'toolu_3',
				is_error: true,
				content: 'Error: Property "durPct" is missing in type ToolCardProps but required',
				content_sha256: 's',
				content_bytes: 1400
			}
		}),
		buildToolUse({
			tool_id: 'toolu_4',
			request_id: 108,
			tool_name: 'Task',
			input: { description: 'Verify card density at 1200 requests', subagent_type: 'test-runner' },
			file_path: null,
			ord: 0,
			result: {
				tool_use_id: 'toolu_4',
				is_error: false,
				content: 'sub-agent completed: card density verified',
				content_sha256: 's',
				content_bytes: 2200
			}
		})
	];

	const DETAIL: SessionDetail = {
		summary: buildSessionSummary({ request_count: FIXTURE_REQUESTS.length }),
		requests: FIXTURE_REQUESTS,
		total_requests: FIXTURE_REQUESTS.length,
		offset: 0,
		limit: 50
	};

	const MAILBOX_SENDER_ID = 'sess-99999999-1111-2222-3333-000000000099';
	const MAILBOX_DELIVERED: MailboxMessage = {
		message_uuid: 'm-story-delivered',
		from_session_uuid: MAILBOX_SENDER_ID,
		to_session_uuid: STORY_SESSION_ID,
		body: 'Reminder: the shared tool-use ledger helper moved into timelineTurnView.ts — rebase before touching TurnCard.',
		queued_at: `${BASE_TIME}08:50Z`,
		claimed_at: `${BASE_TIME}09:05Z`,
		sent_request_id: 107,
		sent_at: `${BASE_TIME}09:08Z`,
		consumed_at: null,
		dead_lettered_at: null,
		delivery_attempts: 1,
		status: 'delivered'
	};
	const MAILBOX_QUEUED: MailboxMessage = {
		message_uuid: 'm-story-queued',
		from_session_uuid: null,
		to_session_uuid: STORY_SESSION_ID,
		body: "Don't touch the exhaustive tool-use switch while I'm mid-edit.",
		queued_at: `${BASE_TIME}20:10Z`,
		claimed_at: null,
		sent_request_id: null,
		sent_at: null,
		consumed_at: null,
		dead_lettered_at: null,
		delivery_attempts: 0,
		status: 'pending'
	};

	function stubApi(): void {
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (/\/api\/sessions\/[^/]+\/tool_uses/.test(url)) {
				const body: ToolUsesResponse = {
					session_id: STORY_SESSION_ID,
					total_count: FIXTURE_TOOL_USES.length,
					offset: 0,
					limit: FIXTURE_TOOL_USES.length,
					rows: FIXTURE_TOOL_USES
				};
				return new Response(JSON.stringify(body), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (/\/api\/requests\/\d+\/body\/response/.test(url)) {
				return new Response(JSON.stringify({ text: null, byte_size: 0, has_body: false }), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (/\/api\/requests\/\d+\/body\/request(_sent)?$/.test(url)) {
				return new Response(JSON.stringify({ text: null, byte_size: 0, has_body: false }), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (/\/api\/requests\/\d+\/manifest/.test(url)) {
				return new Response(
					JSON.stringify({ request_id: 0, has_manifest: false, manifest: null }),
					{
						status: 200,
						headers: { 'content-type': 'application/json' }
					}
				);
			}
			if (/\/api\/sessions\/[^/]+(\?|$)/.test(url)) {
				return new Response(JSON.stringify(DETAIL), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (/\/api\/sessions\/[^/]+\/messages/.test(url)) {
				const body = {
					session_id: STORY_SESSION_ID,
					inbox: [MAILBOX_DELIVERED, MAILBOX_QUEUED],
					outbox: []
				};
				return new Response(JSON.stringify(body), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	const { Story } = defineMeta({
		title: 'Shell/SessionTimeline',
		component: SessionTimelineHarness,
		tags: ['autodocs'],
		argTypes: { detailStore: { control: false } }
	});
</script>

<Story
	name="Default"
	args={{
		detailStore: buildMockDetailStore({ detail: DETAIL, operatorTurns: FIXTURE_TURNS }),
		now: STATIC_NOW
	}}
	loaders={[
		async () => {
			localStorage.clear();
			stubApi();
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

		await waitFor(() => canvas.getAllByText('Latest output').length > 0, {
			timeout: 3000
		});

		await waitFor(() => expect(canvas.getByText('Incoming message')).toBeInTheDocument());
		await expect(canvas.getByText('Queued message')).toBeInTheDocument();

		await expect(canvas.getAllByText(/quota probe/i).length).toBeGreaterThan(0);

		const search = canvas.getByPlaceholderText(/Search prompts/i);
		await userEvent.type(search, 'sub-agent');
		await waitFor(() => expect(canvas.getByText(/of 3 turns match/i)).toBeInTheDocument());

		await expect(canvas.getByText('Incoming message')).toBeInTheDocument();
		await expect(canvas.getByText('Queued message')).toBeInTheDocument();

		await waitFor(() => expect(canvas.queryByText(/quota probe/i)).not.toBeInTheDocument());

		await userEvent.clear(search);
		await waitFor(() => expect(canvas.queryByText(/turns match/i)).not.toBeInTheDocument());
		await waitFor(() => expect(canvas.getAllByText(/quota probe/i).length).toBeGreaterThan(0));

		await userEvent.type(search, 'zzz-nonexistent-prompt');
		await waitFor(() =>
			expect(canvas.getByText('No turns match the current filter.')).toBeInTheDocument()
		);
		await userEvent.click(canvas.getByRole('button', { name: /clear filters/i }));
		await waitFor(() =>
			expect(canvas.queryByText('No turns match the current filter.')).not.toBeInTheDocument()
		);
		await waitFor(() => canvas.getAllByText('Latest output').length > 0);

		const jumpInput = canvas.getByPlaceholderText('req');
		await userEvent.type(jumpInput, '7');
		await userEvent.keyboard('{Enter}');
		await waitFor(() => expect(canvas.getAllByText(/pnpm run check/).length).toBeGreaterThan(0));
		await waitFor(() => expect(canvas.getByText('INPUT')).toBeInTheDocument());

		await userEvent.click(canvas.getByRole('button', { name: /collapse all/i }));
		await waitFor(() => expect(canvas.queryAllByText('Reply').length).toBe(0));

		await userEvent.click(canvas.getByRole('button', { name: /expand all/i }));
		await waitFor(() => expect(canvas.queryAllByText('Reply').length).toBeGreaterThan(0));
	}}
>
	{#snippet template(args)}
		<div style="width: 920px; height: 820px; background: var(--surface-bg); overflow: auto;">
			<SessionTimelineHarness
				detailStore={args.detailStore}
				now={args.now}
				scrollContainer={undefined}
			/>
		</div>
	{/snippet}
</Story>
