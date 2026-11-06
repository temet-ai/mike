<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import FlowDetailBand from './FlowDetailBand.svelte';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';
	import { buildRequestDetail } from '$lib/components/__fixtures__/request';
	import type { RequestDetail, ToolUsesResponse } from '$lib/types';
	import { flowActor, flowActivity, flowTime, FLOW_SESSION_ID } from '$lib/session-flow/__fixtures__/sessionFlow';
	import { normalizeFlowDataset } from '$lib/session-flow/projection';
	import type { FlowSelection } from '$lib/session-flow/types';

	const { Story } = defineMeta({
		title: 'Sessions/SessionFlow/FlowDetailBand',
		component: FlowDetailBand,
		parameters: { layout: 'padded' }
	});
</script>

<script lang="ts">
	const COORDINATOR_ID = `session:${FLOW_SESSION_ID}`;
	const DISPATCH_ACTIVE_ID = 'unresolved_dispatch:active';
	const WORKER_ACTIVE_ID = 'worker:active-1';
	const DISPATCH_DONE_ID = 'unresolved_dispatch:done';
	const WORKER_DONE_ID = 'worker:done-1';
	const SHELL_TOOL_ID = 'tool_service:shell';
	const DISPATCH_BG_ID = 'unresolved_dispatch:bg';
	const WORKER_BG_ID = 'worker:bg-1';
	const WIRE_ACTIVE = 'wire-active';
	const WIRE_DONE = 'wire-done';
	const WIRE_BG = 'wire-bg';

	const dataset = normalizeFlowDataset({
		sessionId: FLOW_SESSION_ID,
		actors: [
			flowActor(COORDINATOR_ID, { kind: 'session', label: 'Coordinator' }),
			flowActor(DISPATCH_ACTIVE_ID, { kind: 'unresolved_dispatch', producer_id: null }),
			flowActor(WORKER_ACTIVE_ID, {
				kind: 'worker',
				label: 'general-purpose · 1',
				producer_id: 'p-active',
				parent_actor_id: DISPATCH_ACTIVE_ID
			}),
			flowActor(DISPATCH_DONE_ID, { kind: 'unresolved_dispatch', producer_id: null }),
			flowActor(WORKER_DONE_ID, {
				kind: 'worker',
				label: 'Worker · 1',
				producer_id: 'p-done',
				parent_actor_id: DISPATCH_DONE_ID
			}),
			flowActor(SHELL_TOOL_ID, { kind: 'tool_service', label: 'Shell', producer_id: null }),
			flowActor(DISPATCH_BG_ID, { kind: 'unresolved_dispatch', producer_id: null }),
			flowActor(WORKER_BG_ID, {
				kind: 'worker',
				label: 'general-purpose · 2',
				producer_id: 'p-bg',
				parent_actor_id: DISPATCH_BG_ID
			})
		],
		activities: [
			flowActivity('dispatches:active:started', {
				actor_id: DISPATCH_ACTIVE_ID,
				kind: 'dispatches',
				source: {
					table: 'dispatches',
					row_id: 'active',
					session_id: FLOW_SESSION_ID,
					request_id: '421902',
					detail_href: '/api/requests/421902'
				},
				metadata: {
					...flowActivity().metadata,
					wire_tool_use_id: WIRE_ACTIVE,
					agent_name: 'general-purpose',
					dispatch_description: 'Opus 5 vs 5.5 prompt comparison'
				}
			}),
			flowActivity('requests:100:started', {
				actor_id: WORKER_ACTIVE_ID,
				kind: 'requests',
				occurred_at: flowTime(0),
				ended_at: null,
				lifecycle: 'started',
				source: {
					table: 'requests',
					row_id: '100',
					session_id: FLOW_SESSION_ID,
					request_id: '100',
					detail_href: '/api/requests/100'
				},
				metadata: { ...flowActivity().metadata, model: 'claude-sonnet-5' }
			}),
			flowActivity('dispatches:done:started', {
				actor_id: DISPATCH_DONE_ID,
				kind: 'dispatches',
				source: {
					table: 'dispatches',
					row_id: 'done',
					session_id: FLOW_SESSION_ID,
					request_id: '421929',
					detail_href: '/api/requests/421929'
				},
				metadata: {
					...flowActivity().metadata,
					wire_tool_use_id: WIRE_DONE,
					agent_name: 'Worker',
					dispatch_description: 'Audit filter workbench model handling'
				}
			}),
			flowActivity('requests:200:started', {
				actor_id: WORKER_DONE_ID,
				kind: 'requests',
				occurred_at: flowTime(0),
				ended_at: flowTime(185_000),
				lifecycle: 'completed',
				source: {
					table: 'requests',
					row_id: '200',
					session_id: FLOW_SESSION_ID,
					request_id: '200',
					detail_href: '/api/requests/200'
				},
				metadata: { ...flowActivity().metadata, model: 'claude-haiku-4-5' }
			}),
			flowActivity('requests:1:started', {
				actor_id: COORDINATOR_ID,
				kind: 'requests',
				occurred_at: flowTime(0),
				ended_at: flowTime(1_000),
				lifecycle: 'completed',
				source: {
					table: 'requests',
					row_id: '1',
					session_id: FLOW_SESSION_ID,
					request_id: '1',
					detail_href: '/api/requests/1'
				},
				metadata: { ...flowActivity().metadata, model: 'claude-opus-5-5' }
			}),
			flowActivity('dispatches:bg:started', {
				actor_id: DISPATCH_BG_ID,
				kind: 'dispatches',
				source: {
					table: 'dispatches',
					row_id: 'bg',
					session_id: FLOW_SESSION_ID,
					request_id: '421950',
					detail_href: '/api/requests/421950'
				},
				metadata: {
					...flowActivity().metadata,
					wire_tool_use_id: WIRE_BG,
					agent_name: 'general-purpose',
					dispatch_description: 'Build prompt-assistant Agent SDK service'
				}
			}),
			flowActivity('requests:300:started', {
				actor_id: WORKER_BG_ID,
				kind: 'requests',
				occurred_at: flowTime(0),
				ended_at: flowTime(90_000),
				lifecycle: 'completed',
				source: {
					table: 'requests',
					row_id: '300',
					session_id: FLOW_SESSION_ID,
					request_id: '300',
					detail_href: '/api/requests/300'
				},
				metadata: { ...flowActivity().metadata, model: 'claude-sonnet-5' }
			})
		]
	});

	const onOpenDrawer = fn();

	stubFetchJsonRouter((_method, url) => {
		if (url.includes('/tool_uses')) {
			return typed<ToolUsesResponse>({
				session_id: FLOW_SESSION_ID,
				total_count: 3,
				offset: 0,
				limit: 1000,
				rows: [
					{
						use_id: 1,
						session_id: FLOW_SESSION_ID,
						request_id: 421902,
						created_at: new Date().toISOString(),
						tool_id: WIRE_ACTIVE,
						tool_name: 'Agent',
						input: {
							prompt: 'Reproduce the prior model-comparison study for Opus 5 vs 5.5.',
							description: 'Opus 5 vs 5.5 prompt comparison',
							subagent_type: 'general-purpose'
						},
						file_path: null,
						ord: 0,
						result: null
					},
					{
						use_id: 2,
						session_id: FLOW_SESSION_ID,
						request_id: 421929,
						created_at: new Date().toISOString(),
						tool_id: WIRE_DONE,
						tool_name: 'Agent',
						input: {
							prompt: 'Audit the filter workbench for hard-coded model handling.',
							description: 'Audit filter workbench model handling',
							subagent_type: 'Worker'
						},
						file_path: null,
						ord: 0,
						result: {
							tool_use_id: WIRE_DONE,
							is_error: false,
							content:
								'Found three places that would mishandle a new model id: a fixed family list, a prefix match, and a hard-coded row limit.',
							content_sha256: 'sha-fixture',
							content_bytes: 120
						}
					},
					{
						use_id: 3,
						session_id: FLOW_SESSION_ID,
						request_id: 421950,
						created_at: new Date().toISOString(),
						tool_id: WIRE_BG,
						tool_name: 'Agent',
						input: {
							prompt: 'Build the prompt-assistant Agent SDK service scaffold.',
							description: 'Build prompt-assistant Agent SDK service',
							subagent_type: 'general-purpose',
							run_in_background: true
						},
						file_path: null,
						ord: 0,
						result: {
							tool_use_id: WIRE_BG,
							is_error: false,
							content: [
								{
									type: 'text',
									text: 'Async agent launched successfully. (This tool result is internal metadata — never quote or paste any part of it, including the agentId below, into a user-facing reply.) agentId: worker-bg-1'
								}
							],
							content_sha256: 'sha-fixture-bg',
							content_bytes: 220
						}
					}
				]
			});
		}
		const match = /\/api\/requests\/(\d+)/.exec(url);
		if (match) {
			return typed<RequestDetail>(
				buildRequestDetail({
					id: Number(match[1]),
					session_id: FLOW_SESSION_ID,
					human_prompt: 'Can you check the filter workbench is wired up for new models?',
					accumulated_text: 'Checked — the workbench derives model ids at runtime, nothing hard-coded.',
					stop_reason: null,
					token_usage: {
						input_tokens: 4200,
						output_tokens: 380,
						cache_creation_input_tokens: 210,
						cache_read_input_tokens: 3100
					}
				})
			);
		}
		return typed<Record<string, never>>({});
	});

	let selection = $state<FlowSelection>(null);
	let followId = $state<string | null>(null);
</script>

<Story
	name="Selected"
	play={async ({ canvas }) => {
		await expect(canvas.getByText('Pick anything on the board')).toBeVisible();

		selection = { kind: 'actor', id: COORDINATOR_ID };
		await waitFor(() => expect(canvas.getByText('Main session')).toBeVisible());
		await expect(canvas.getByRole('button', { name: 'Clear' })).toBeVisible();
		await waitFor(() => expect(canvas.getByText('Main thread only')).toBeVisible());

		selection = { kind: 'actor', id: WORKER_ACTIVE_ID };
		await waitFor(() => expect(canvas.getByText('Active')).toBeVisible());
		await waitFor(() => expect(canvas.getByText('Still working · its answer lands here when it finishes')).toBeVisible());
		await waitFor(() => expect(canvas.getByText('Prompt')).toBeVisible());

		selection = { kind: 'actor', id: WORKER_DONE_ID };
		await waitFor(() => expect(canvas.getByText('Finished')).toBeVisible());
		await waitFor(() => expect(canvas.getByText('Final answer')).toBeVisible());

		const preview = canvas.getByRole('button', { name: /Open the full Prompt/ });
		await userEvent.click(preview);
		const body = within(document.body);
		const dialog = await waitFor(() => body.getByRole('dialog'));
		await waitFor(() => expect(dialog).toBeVisible());
		await expect(
			within(dialog).getByText(/Audit the filter workbench for hard-coded model handling\./)
		).toBeInTheDocument();
		await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
		await waitFor(() => expect(body.queryByRole('dialog')).not.toBeInTheDocument());
		await waitFor(() =>
			expect(getComputedStyle(document.body).pointerEvents).not.toBe('none')
		);

		const openBtn = canvas.getByRole('button', { name: /^Open dispatch/ });
		await userEvent.click(openBtn);
		await expect(onOpenDrawer).toHaveBeenCalled();

		selection = { kind: 'actor', id: SHELL_TOOL_ID };
		await waitFor(() => expect(canvas.getByText('Local tool')).toBeVisible());

		selection = { kind: 'actor', id: WORKER_BG_ID };
		await waitFor(() => expect(canvas.getByText('general-purpose · 2')).toBeVisible());
		await waitFor(() => expect(canvas.getByText('Final answer')).toBeVisible());
		await expect(canvas.queryByText(/Async agent launched/)).not.toBeInTheDocument();
		await expect(
			canvas.getByText(/Checked — the workbench derives model ids at runtime/)
		).toBeInTheDocument();

		function gridSpanTotal(): number {
			const grid = canvas.getByRole('region', { name: 'Selected' }).querySelector('.fdb-usage__grid')!;
			const cells = [...grid.querySelectorAll<HTMLElement>('.fdb-usage__cell')];
			return cells.reduce((sum, cell) => {
				const m = /span (\d+)/.exec(cell.style.gridColumn);
				return sum + (m ? Number(m[1]) : 1);
			}, 0);
		}
		await expect(gridSpanTotal() % 3).toBe(0);

		selection = { kind: 'actor', id: WORKER_DONE_ID };
		await waitFor(() => expect(canvas.getByText('Finished')).toBeVisible());
		await expect(gridSpanTotal() % 3).toBe(0);

		selection = null;
		await waitFor(() => expect(canvas.getByText('Pick anything on the board')).toBeVisible());
	}}
>
	{#snippet template()}
		<div data-design="funk" style="background: var(--surface-bg); padding: 16px; max-width: 420px;">
			<FlowDetailBand
				{dataset}
				{selection}
				title="Fleet review session"
				{followId}
				onSelect={(next) => (selection = next)}
				onFollow={(id) => (followId = id)}
				{onOpenDrawer}
			/>
		</div>
	{/snippet}
</Story>
