<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import TurnCardHarness from './__harness__/TurnCardHarness.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import type { OperatorTurnDto, ToolUseDto } from '$lib/types';
	import { turnCollapseKey } from './timelineTurnView';

	const TURN: OperatorTurnDto = {
		turn_id: 2,
		session_id: 'storybook-session',
		project_id: 1,
		start_request_id: 105,
		end_request_id: 109,
		start_request_number: 6,
		end_request_number: 10,
		user_prompt:
			'The whole thing is too fat and does not fit the style — and I dislike the generic left-border accent on rows. How does it hold up at a hundred or a thousand requests?',
		final_assistant_text:
			'Rebuilt around dense, readable tool cards — request #, tool, target, a magnitude bar and status — matching the app amber, hairline, monospace chrome. First cut failed type-check on a missing prop; fixed, green now.',
		tool_call_count: 2,
		tool_error_count: 1,
		files_touched: [],
		tools_used: ['Bash', 'Task'],
		agents_dispatched: 1,
		weasel_findings_count: 0,
		total_input_tokens: 61400,
		total_output_tokens: 5200,
		duration_ms: 9800,
		outcome: 'corrected',
		outcome_evidence: null,
		classified_at: '2026-07-02T14:09:10Z',
		created_at: '2026-07-02T14:09:00Z',
		updated_at: '2026-07-02T14:09:10Z',
		stop_response:
			'Rebuilt around dense, readable tool cards — request #, tool, target, a magnitude bar and status — matching the app amber, hairline, monospace chrome. First cut failed type-check on a missing prop; fixed, green now.',
		stop_reason: 'end_turn',
		stop_request_id: 109,
		interrupted: false
	};

	const EVENT_TURN: OperatorTurnDto = {
		turn_id: 3,
		session_id: 'storybook-session',
		project_id: 1,
		start_request_id: 110,
		end_request_id: 110,
		start_request_number: 11,
		end_request_number: 11,
		user_prompt:
			'Another Claude session sent a message:\n<teammate-message teammate_id="phase-a-reducer" summary="Phase A file-read reducer implemented, gates green, replay done.">\nPhase A file-read reducer implemented, gates green, replay done.\n</teammate-message>',
		final_assistant_text:
			'The `ManifestBuilder` is correct: `record_applied` hashes new_bytes once and advances the running hash; `finish` sets sent_sha256 and leaves verification to `verify_chain()`.',
		tool_call_count: 0,
		tool_error_count: 0,
		files_touched: [],
		tools_used: [],
		agents_dispatched: 0,
		weasel_findings_count: 0,
		total_input_tokens: 2300,
		total_output_tokens: 28000,
		duration_ms: 5000,
		outcome: 'unknown',
		outcome_evidence: null,
		classified_at: '2026-07-02T14:15:05Z',
		created_at: '2026-07-02T14:15:00Z',
		updated_at: '2026-07-02T14:15:05Z',
		stop_response:
			'The `ManifestBuilder` is correct: `record_applied` hashes new_bytes once and advances the running hash; `finish` sets sent_sha256 and leaves verification to `verify_chain()`.',
		stop_reason: 'end_turn',
		stop_request_id: 110,
		interrupted: false,
		prompt_kind: 'teammate-message'
	};

	const TOOL_CALLS: ToolUseDto[] = [
		{
			request_id: 106,
			session_id: 'storybook-session',
			created_at: '2026-07-02T14:09:04Z',
			tool_id: 'toolu_1',
			tool_name: 'Bash',
			input: { command: 'pnpm run check' },
			file_path: null,
			ord: 0,
			result: {
				tool_use_id: 'toolu_1',
				is_error: true,
				content: 'Error: Property "durPct" is missing in type ToolCardProps but required',
				content_sha256: 's',
				content_bytes: 1400
			}
		},
		{
			request_id: 108,
			session_id: 'storybook-session',
			created_at: '2026-07-02T14:09:12Z',
			tool_id: 'toolu_2',
			tool_name: 'Task',
			input: { description: 'Verify card density at 1200 requests', subagent_type: 'test-runner' },
			file_path: null,
			ord: 0,
			result: {
				tool_use_id: 'toolu_2',
				is_error: false,
				content: 'sub-agent completed: card density verified across 1200 requests',
				content_sha256: 's',
				content_bytes: 2200
			}
		}
	];

	function stubResponseBodyFetch(): void {
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
			const body = url.includes('/api/eval/vocabulary')
				? { axes: [] }
				: url.includes('/api/eval/labels')
					? { labels: [], next_before_id: null, verdict_now: null }
					: { text: null, byte_size: 0, has_body: false };
			return new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		};
	}

	const { Story } = defineMeta({
		title: 'Shell/TurnCard',
		component: TurnCardHarness,
		tags: ['autodocs'],
		args: {
			turn: TURN,
			eventTurn: EVENT_TURN,
			index: 2,
			isFirst: false,
			toolCalls: TOOL_CALLS,
			ledgerUnavailable: false,
			live: false,
			now: Date.now(),
			defaultExpanded: true,
			query: '',
			toolFilter: 'all',
			forcedOpen: null,
			flash: false,
			jumpTargetRequestId: null,
			requestNumberFor: (id: number) => id,
			statusFor: () => 'Complete',
			onManualToggle: fn(),
			onOpenDrawer: fn(),
			onJumpSelf: fn()
		}
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			localStorage.clear();
			localStorage.setItem(
				'proxy-inspector:collapsed',
				JSON.stringify([turnCollapseKey(EVENT_TURN)])
			);
			stubResponseBodyFetch();
			return {};
		}
	]}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText('Tools')).toBeInTheDocument();
		await expect(canvas.getByText('Reply')).toBeInTheDocument();

		await userEvent.click(canvas.getByText(/too fat and does not fit the style/i));
		await waitFor(async () => {
			await expect(canvas.queryByText('Tools')).not.toBeInTheDocument();
		});

		await userEvent.click(canvas.getByRole('button', { name: /reply preview/i }));
		await expect(canvas.getByText('Tools')).toBeInTheDocument();

		const collapseToggle = canvas.getByRole('button', { name: /collapse operator turn 2/i });
		await userEvent.click(collapseToggle);
		await waitFor(async () => {
			await expect(canvas.queryByText('Tools')).not.toBeInTheDocument();
		});
		await userEvent.click(canvas.getByRole('button', { name: /^expand operator turn 2$/i }));
		await expect(canvas.getByText('Tools')).toBeInTheDocument();

		const errorToggle = canvas.getByRole('button', { name: /expand bash call/i });
		await userEvent.click(errorToggle);
		await expect(canvas.getByText('ERROR')).toBeInTheDocument();

		await userEvent.click(canvas.getByTitle(/requests #6/i));
		await expect(args.onJumpSelf).toHaveBeenCalled();

		await expect(canvas.getByText('TEAMMATE MSG')).toBeInTheDocument();
		await expect(canvas.getByText('phase-a-reducer')).toBeInTheDocument();
		await expect(canvas.queryByText(/Injected prompt/i)).not.toBeInTheDocument();

		const eventRoot = canvasElement.querySelector('[data-slot="session-event-row"]');
		await expect(eventRoot).not.toBeNull();
		const hasOrdinal = Array.from(eventRoot?.querySelectorAll('span') ?? []).some((el) =>
			/^\d{2}$/.test(el.textContent?.trim() ?? '')
		);
		await expect(hasOrdinal).toBe(false);

		const eventStyle = eventRoot ? getComputedStyle(eventRoot) : null;
		await expect(eventStyle?.borderRadius).toBe('0px');
		await expect(eventStyle?.borderTopWidth).toBe('0px');
		await expect(eventStyle?.borderLeftWidth).toBe('0px');
		await expect(eventStyle?.borderRightWidth).toBe('0px');

		const eventToggle = canvas.getByRole('button', { name: /expand teammate-message event/i });
		await userEvent.click(eventToggle);
		await expect(canvas.getByText(/Injected prompt/i)).toBeInTheDocument();
		await expect(canvas.getByText('Result')).toBeInTheDocument();
		await expect(canvas.getByText('Tools')).toBeInTheDocument();
		await userEvent.click(canvas.getByRole('button', { name: /collapse teammate-message event/i }));
		await expect(canvas.queryByText(/Injected prompt/i)).not.toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div style="width: 720px; background: var(--surface-bg); padding: 12px;">
			<TurnCardHarness {...args} />
		</div>
	{/snippet}
</Story>
