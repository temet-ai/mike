<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import { setContext } from 'svelte';
	import FilterWorkbench from './FilterWorkbench.svelte';
	import { FILTER_RULES_STORE_KEY } from '$lib/context-keys';
	import { createMockFilterRulesStore } from '$lib/storybook/mockFilterRulesStore.svelte';
	import type { FilterRuleDto, RequestDetail, SystemBlock } from '$lib/types';

	const REQUEST_ID = 42;

	function sysBlock(index: number, text: string): SystemBlock {
		return { index, text, cache_control: null, composition_match: null };
	}

	const BLOCK_0 =
		"You are Claude Code, Anthropic's official CLI for Claude. Refuse destructive requests without asking first.";
	const BLOCK_1 = '<system-reminder>\nPast sessions are memory too.\n</system-reminder>';

	const DETAIL = {
		id: REQUEST_ID,
		session_id: 'session-fw-story',
		model: 'claude-sonnet-5',
		turn_origin: 'primary_first',
		subagent_persona: null,
		system_blocks: {
			prompt_sha256: 'a'.repeat(64),
			composition_id: null,
			composition_version: null,
			blocks: [sysBlock(0, BLOCK_0), sysBlock(1, BLOCK_1)]
		}
	} as RequestDetail;

	function buildRule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
		return {
			// A stringified numeric id, matching the real contract (`FilterRuleDto.id` is always
			// `r.id.to_string()` server-side — see `crates/mike/src/api/handlers/filter_rule_dto.rs`)
			// so it lines up with `firing_rule_ids` below (numeric on the wire).
			id: '101',
			kind: 'text_substring',
			value: 'Refuse destructive requests without asking first.',
			action: 'replace',
			text: 'Confirm proceeding through a quick approval step.',
			title: null,
			enabled: true,
			source: 'ui',
			updated_at: '2026-04-17T12:00:00Z',
			sort_order: 1,
			trigger_token: null,
			fires_24h: 2,
			fires_7d: 9,
			last_fired_at: '2026-09-20T09:00:00Z',
			...overrides
		};
	}

	const RESULT_BLOCKS = [
		"You are Claude Code, Anthropic's official CLI for Claude. Confirm proceeding through a quick approval step.",
		BLOCK_1
	];

	/** The size chip's expected text once the fix lands: system-prompt-only sums on both sides,
	 *  never the mocked dry-run's inflated whole-request-body `original_len`/`filtered_len`
	 *  above (round-4 defect #3). */
	const EXPECTED_TOTAL_CHARS = BLOCK_0.length + BLOCK_1.length;
	const EXPECTED_AFTER_CHARS = RESULT_BLOCKS[0]!.length + RESULT_BLOCKS[1]!.length;

	function stubFetch(): void {
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.startsWith('/api/projects')) {
				return new Response(JSON.stringify([]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.endsWith('/api/filter/rules')) {
				return new Response(JSON.stringify([buildRule()]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/assistant/health')) {
				return new Response(
					JSON.stringify({
						ok: true,
						enabled: true,
						model: 'claude-sonnet-5',
						effort: 'medium',
						may_apply: 'suggest',
						spent_today_usd: 0.02,
						daily_budget_usd: 1
					}),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}
			if (url.includes('/api/filter/rules/dry-run')) {
				return new Response(
					JSON.stringify({
						request_id: REQUEST_ID,
						changed: true,
						project_path: '/tmp',
						firing_rule_ids: [101],
						// Deliberately different from the system-prompt-only sums below: the dry-run
						// engine's original_len/filtered_len cover the WHOLE request body (messages
						// included), not just the system prompt — the size chip must ignore these two
						// fields and sum filtered_system itself instead (round-4 defect #3).
						original_len: BLOCK_0.length + BLOCK_1.length + 50_000,
						filtered_len: RESULT_BLOCKS[0]!.length + RESULT_BLOCKS[1]!.length + 50_000,
						filtered_system: RESULT_BLOCKS.map((text) => ({ type: 'text', text })),
						filtered_messages: null
					}),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}
			if (url.includes(`/api/requests/${REQUEST_ID}`)) {
				return new Response(JSON.stringify(DETAIL), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404, statusText: url });
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Filter/FilterWorkbench',
		component: FilterWorkbench,
		tags: ['autodocs']
	});
	const ALPHA_PROJECT_PATH = '/mnt/data/projects/alpha';
	const ALPHA_REQUEST_ID = 99;
	const BETA_PROJECT_PATH = '/mnt/data/projects/beta';

	const ALPHA_BLOCK_0 =
		"You are Claude Code, Anthropic's official CLI for Claude. Refuse destructive requests without asking first — Alpha's own capture.";

	const ALPHA_DETAIL = {
		id: ALPHA_REQUEST_ID,
		session_id: 'session-fw-alpha',
		model: 'claude-sonnet-5',
		turn_origin: 'primary_first',
		subagent_persona: null,
		system_blocks: {
			prompt_sha256: 'b'.repeat(64),
			composition_id: null,
			composition_version: null,
			blocks: [sysBlock(0, ALPHA_BLOCK_0)]
		}
	} as RequestDetail;

	function stubProjectPickFetch(): void {
		globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
			const rawUrl = typeof input === 'string' ? input : input.toString();
			const url = new URL(rawUrl, 'http://localhost');

			if (url.pathname === '/api/projects') {
				return new Response(
					JSON.stringify([
						{
							project_id: 1,
							project_uuid: 'a',
							project_path: ALPHA_PROJECT_PATH,
							session_count: 5,
							request_count: 30,
							first_seen: '2026-01-01T00:00:00Z',
							last_seen: '2026-09-20T00:00:00Z',
							pinned: false,
							custom_name: 'Alpha'
						},
						{
							project_id: 2,
							project_uuid: 'b',
							project_path: BETA_PROJECT_PATH,
							session_count: 1,
							request_count: 2,
							first_seen: '2026-01-01T00:00:00Z',
							last_seen: '2026-09-10T00:00:00Z',
							pinned: false,
							custom_name: 'Beta'
						}
					]),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}
			if (url.pathname === '/api/filter/rules') {
				return new Response(JSON.stringify([]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.pathname.includes('/api/assistant/health')) {
				return new Response(
					JSON.stringify({
						ok: true,
						enabled: true,
						model: 'claude-sonnet-5',
						effort: 'medium',
						may_apply: 'suggest',
						spent_today_usd: 0.02,
						daily_budget_usd: 1
					}),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}
			if (url.pathname === '/api/prompts/classes/main' && url.searchParams.get('project_path') === ALPHA_PROJECT_PATH) {
				return new Response(
					JSON.stringify({
						lane: 'main',
						subagent_persona: null,
						observation_count: 1,
						first_seen: '2026-09-01T00:00:00Z',
						last_seen: '2026-09-20T00:00:00Z',
						model_breakdown: [],
						sample_blocks: null,
						registration: { registered: false, composition_id: null, label: null, mode: null, agent_definition_path: null },
						recent_observations: [
							{
								request_id: String(ALPHA_REQUEST_ID),
								session_id: 'session-fw-alpha',
								created_at: '2026-09-20T00:00:00Z',
								content_sha256: 'b'.repeat(64),
								altered: false,
								model: 'claude-sonnet-5'
							}
						],
						observations_total_count: 1,
						observations_offset: 0,
						observations_limit: 20
					}),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}
			if (url.pathname === '/api/prompts/classes/main' && url.searchParams.get('project_path') === BETA_PROJECT_PATH) {
				return new Response(JSON.stringify({ error: 'no prompt class observed' }), {
					status: 404,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.pathname === '/api/prompts/classes' && url.searchParams.get('project_path') === BETA_PROJECT_PATH) {
				return new Response(JSON.stringify([]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.pathname === `/api/requests/${ALPHA_REQUEST_ID}`) {
				return new Response(JSON.stringify(ALPHA_DETAIL), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.pathname === `/api/requests/${REQUEST_ID}`) {
				return new Response(JSON.stringify(DETAIL), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.pathname === '/api/filter/rules/dry-run') {
				const body = init?.body ? (JSON.parse(init.body as string) as { request_id: number }) : { request_id: 0 };
				const blocks = body.request_id === ALPHA_REQUEST_ID ? [ALPHA_BLOCK_0] : [BLOCK_0, BLOCK_1];
				return new Response(
					JSON.stringify({
						request_id: body.request_id,
						changed: false,
						project_path: '/tmp',
						firing_rule_ids: [],
						original_len: blocks.join('').length,
						filtered_len: blocks.join('').length,
						filtered_system: blocks.map((text) => ({ type: 'text', text })),
						filtered_messages: null
					}),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}
			return new Response('Not Found', { status: 404, statusText: rawUrl });
		};
	}
</script>

<script lang="ts">
	const store = createMockFilterRulesStore([buildRule()]);
	// svelte-ignore state_referenced_locally -- one-time context registration for the story
	setContext(FILTER_RULES_STORE_KEY, store);
</script>

<!--
	Multiple stories on this one component are an explicit operator-named exception to the
	single-representative rule (`.claude/rules/frontend-development.md`): "Stories for
	the layout at 1920, 1440 and 1200 wide and for collapse/expand." `Layout1920` also carries the
	full functional coverage the previous single "Default" story had (lens tabs, the edits list,
	the dry-run size chip, the combined Read Only toggle) — it is the canonical, fully-open state,
	not a layout-only smoke test — so that coverage isn't lost to the split.
-->

<Story
	name="Layout1920"
	loaders={[
		async () => {
			stubFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(await canvas.findByText('Filter workbench')).toBeInTheDocument();
		await expect(canvas.getByRole('tab', { name: 'As sent' })).toBeInTheDocument();
		await expect(canvas.getByRole('tab', { name: 'With my edits' })).toBeInTheDocument();
		await expect(canvas.getByRole('tab', { name: 'Final' })).toBeInTheDocument();

		// At 1920px every column starts open — no collapsed rail for any of the three.
		await expect(canvas.queryByRole('button', { name: 'Expand Outline' })).toBeNull();
		await expect(canvas.queryByRole('button', { name: 'Expand Your edits' })).toBeNull();
		await expect(canvas.queryByRole('button', { name: 'Expand Prompt assistant' })).toBeNull();
		await expect(await canvas.findByRole('region', { name: 'Prompt navigator' })).toBeInTheDocument();
		await expect(await canvas.findByRole('region', { name: 'Prompt assistant' })).toBeInTheDocument();

		// The prompt pane's own per-paragraph quick-edit marker can carry this same derived
		// title text, so this is found scoped to the edits sidebar section rather than via a
		// bare text query that can match either.
		const editsPanel = within(await canvas.findByRole('region', { name: 'Your edits' }));
		await expect(
			await editsPanel.findByText('Rewrites “Refuse destructive requests without asking first.”')
		).toBeInTheDocument();
		await expect(canvas.queryByText('101')).toBeNull();

		// The size chip shows a system-prompt-only comparison — not the mocked dry-run's
		// inflated whole-request-body original_len/filtered_len (round-4 defect #3).
		await expect(
			await canvas.findByText(
				`${EXPECTED_TOTAL_CHARS.toLocaleString()} → ${EXPECTED_AFTER_CHARS.toLocaleString()} chars`
			)
		).toBeInTheDocument();

		const resultTab = canvas.getByRole('tab', { name: 'Final' });
		await userEvent.click(resultTab);
		await expect(
			await canvas.findByText(/Confirm proceeding through a quick approval step/)
		).toBeInTheDocument();

		// "Read only" collapses BOTH the edits and the assistant columns to their rails together
		// (spec) — the section itself unmounts (its region disappears), replaced by a rail
		// button, not just visually hidden.
		const readOnly = await canvas.findByRole('button', { name: 'Read only' });
		await userEvent.click(readOnly);
		await expect(canvas.queryByRole('region', { name: 'Your edits' })).toBeNull();
		await expect(canvas.queryByRole('region', { name: 'Prompt assistant' })).toBeNull();
		await expect(await canvas.findByRole('button', { name: 'Expand Your edits' })).toBeInTheDocument();
		await expect(
			await canvas.findByRole('button', { name: 'Expand Prompt assistant' })
		).toBeInTheDocument();

		await userEvent.click(await canvas.findByRole('button', { name: 'Show edits' }));
		await expect(await canvas.findByRole('region', { name: 'Your edits' })).toBeInTheDocument();
		await expect(
			await canvas.findByRole('region', { name: 'Prompt assistant' })
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[820px] w-[1920px] overflow-hidden bg-[var(--surface-bg)]">
			<FilterWorkbench requestId={REQUEST_ID} blockParam={null} prefillParam={null} ruleParam={null} />
		</div>
	{/snippet}
</Story>

<Story
	name="Layout1440"
	loaders={[
		async () => {
			stubFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('Filter workbench')).toBeInTheDocument();

		// Below the 1680px all-open floor: the outline starts collapsed to its rail...
		await expect(await canvas.findByRole('button', { name: 'Expand Outline' })).toBeInTheDocument();
		await expect(canvas.queryByRole('region', { name: 'Filter workbench outline' })).toBeNull();

		// ...but 1440 is still at/above the 1280px assistant floor, so edits AND the assistant
		// both start open.
		await expect(await canvas.findByRole('region', { name: 'Your edits' })).toBeInTheDocument();
		await expect(await canvas.findByRole('region', { name: 'Prompt assistant' })).toBeInTheDocument();
		await expect(canvas.queryByRole('button', { name: 'Expand Prompt assistant' })).toBeNull();

		// The prompt column keeps its 520px floor rather than being squeezed by the two open
		// side columns at this width.
		const prompt = canvasElement.querySelector('.wb-prompt-slot');
		expect(prompt).not.toBeNull();
		expect(prompt!.getBoundingClientRect().width).toBeGreaterThanOrEqual(520);
	}}
>
	{#snippet template()}
		<div class="h-[820px] w-[1440px] overflow-hidden bg-[var(--surface-bg)]">
			<FilterWorkbench requestId={REQUEST_ID} blockParam={null} prefillParam={null} ruleParam={null} />
		</div>
	{/snippet}
</Story>

<Story
	name="Layout1200"
	loaders={[
		async () => {
			stubFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('Filter workbench')).toBeInTheDocument();

		// Below both floors: the outline AND the assistant start collapsed to their rails.
		await expect(await canvas.findByRole('button', { name: 'Expand Outline' })).toBeInTheDocument();
		await expect(
			await canvas.findByRole('button', { name: 'Expand Prompt assistant' })
		).toBeInTheDocument();

		// Edits has no viewport-based default of its own — it stays open even this narrow.
		await expect(await canvas.findByRole('region', { name: 'Your edits' })).toBeInTheDocument();

		const prompt = canvasElement.querySelector('.wb-prompt-slot');
		expect(prompt).not.toBeNull();
		expect(prompt!.getBoundingClientRect().width).toBeGreaterThanOrEqual(520);
	}}
>
	{#snippet template()}
		<div class="h-[820px] w-[1200px] overflow-hidden bg-[var(--surface-bg)]">
			<FilterWorkbench requestId={REQUEST_ID} blockParam={null} prefillParam={null} ruleParam={null} />
		</div>
	{/snippet}
</Story>

<Story
	name="CollapseExpand"
	loaders={[
		async () => {
			stubFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('Filter workbench')).toBeInTheDocument();

		// At 1440px: below the 1680px outline floor (starts collapsed), at/above the 1280px
		// assistant floor (starts open), and edits has no responsive default (always open).
		await expect(await canvas.findByRole('button', { name: 'Expand Outline' })).toBeInTheDocument();
		await expect(await canvas.findByRole('region', { name: 'Your edits' })).toBeInTheDocument();
		await expect(await canvas.findByRole('region', { name: 'Prompt assistant' })).toBeInTheDocument();

		// Clicking a collapsed column's rail expands it (no caret, the rail itself is the
		// control).
		const outlineRail = await canvas.findByRole('button', { name: 'Expand Outline' });
		await userEvent.click(outlineRail);
		await expect(await canvas.findByRole('region', { name: 'Prompt navigator' })).toBeInTheDocument();
		await expect(canvas.queryByRole('button', { name: 'Expand Outline' })).toBeNull();

		// Clicking the section's own title (not a caret/chevron, not a "Hide" label) collapses
		// it back to a rail.
		const collapseOutline = await canvas.findByRole('button', { name: 'Collapse outline' });
		await userEvent.click(collapseOutline);
		await expect(await canvas.findByRole('button', { name: 'Expand Outline' })).toBeInTheDocument();
		await expect(canvas.queryByRole('region', { name: 'Prompt navigator' })).toBeNull();

		// Same round trip for the assistant, which starts open at 1440px: collapse via its own
		// title first, then re-expand via its rail.
		const collapseAssistant = await canvas.findByRole('button', {
			name: 'Collapse prompt assistant'
		});
		await userEvent.click(collapseAssistant);
		await expect(
			await canvas.findByRole('button', { name: 'Expand Prompt assistant' })
		).toBeInTheDocument();
		await expect(canvas.queryByRole('region', { name: 'Prompt assistant' })).toBeNull();
		const assistantRail = await canvas.findByRole('button', { name: 'Expand Prompt assistant' });
		await userEvent.click(assistantRail);
		await expect(await canvas.findByRole('region', { name: 'Prompt assistant' })).toBeInTheDocument();

		// Same round trip for "Your edits", which also starts open at 1440px.
		const collapseEdits = await canvas.findByRole('button', { name: 'Collapse your edits' });
		await userEvent.click(collapseEdits);
		await expect(await canvas.findByRole('button', { name: 'Expand Your edits' })).toBeInTheDocument();
		const editsRail = await canvas.findByRole('button', { name: 'Expand Your edits' });
		await userEvent.click(editsRail);
		await expect(await canvas.findByRole('region', { name: 'Your edits' })).toBeInTheDocument();

		// The toolbar's "Read only" collapses BOTH edits and the assistant together, distinct
		// from either column's own per-section toggle above.
		const readOnly = await canvas.findByRole('button', { name: 'Read only' });
		await userEvent.click(readOnly);
		await waitFor(async () => {
			await expect(canvas.queryByRole('region', { name: 'Your edits' })).toBeNull();
			await expect(canvas.queryByRole('region', { name: 'Prompt assistant' })).toBeNull();
		});
		await userEvent.click(await canvas.findByRole('button', { name: 'Show edits' }));
		await expect(await canvas.findByRole('region', { name: 'Your edits' })).toBeInTheDocument();
		await expect(await canvas.findByRole('region', { name: 'Prompt assistant' })).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[820px] w-[1440px] overflow-hidden bg-[var(--surface-bg)]">
			<FilterWorkbench requestId={REQUEST_ID} blockParam={null} prefillParam={null} ruleParam={null} />
		</div>
	{/snippet}
</Story>



<Story
	name="ProjectPick"
	loaders={[
		async () => {
			stubProjectPickFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		// The project picker's popover content renders through a portal (outside canvasElement),
		// so its options are queried against document.body.
		const body = within(document.body);
		await expect(await canvas.findByText('Filter workbench')).toBeInTheDocument();

		// Starts on Global, showing the originally-opened request's own prompt.
		await expect(
			await canvas.findByText(/Refuse destructive requests without asking first\./, {
				selector: '.wb-para-text'
			})
		).toBeInTheDocument();

		// Picking a project whose latest capture of the SAME model·class exists redirects the
		// whole prompt pane to THAT project's own capture (spec: "A project = the latest capture
		// of the same model·class from that project").
		const picker = await canvas.findByRole('button', { name: 'Prompt project: Global' });
		await userEvent.click(picker);
		await userEvent.click(await body.findByRole('option', { name: /Alpha/ }));

		await expect(
			await canvas.findByText(/Refuse destructive requests without asking first — Alpha's own capture\./)
		).toBeInTheDocument();
		await expect(
			await canvas.findByRole('button', { name: 'Prompt project: Alpha' })
		).toBeInTheDocument();

		// Picking a project with NO capture of this class shows the fallback message instead of
		// stale content from the previous pick.
		await userEvent.click(await canvas.findByRole('button', { name: 'Prompt project: Alpha' }));
		await userEvent.click(await body.findByRole('option', { name: /Beta/ }));
		await expect(await canvas.findByText(/No capture of .* in Beta/)).toBeInTheDocument();
		await expect(
			canvas.queryByText(/Refuse destructive requests without asking first/)
		).toBeNull();

		// Back to Global restores the originally-opened request's own prompt.
		await userEvent.click(await canvas.findByRole('button', { name: 'Prompt project: Beta' }));
		await userEvent.click(await body.findByRole('option', { name: /Global/ }));
		await expect(
			await canvas.findByText(/Refuse destructive requests without asking first\./, {
				selector: '.wb-para-text'
			})
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[820px] w-[1920px] overflow-hidden bg-[var(--surface-bg)]">
			<FilterWorkbench requestId={REQUEST_ID} blockParam={null} prefillParam={null} ruleParam={null} />
		</div>
	{/snippet}
</Story>
