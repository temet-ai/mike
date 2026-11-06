<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import FilterWorkbenchRouteHarness from './__harness__/FilterWorkbenchRouteHarness.svelte';
	import type { FilterRulesStore } from '$lib/stores/filterRules.svelte';
	import type { FilterRuleDto, RequestDetail, SystemBlock } from '$lib/types';

	const SOURCE_REQUEST_ID = 42;

	function sysBlock(index: number, text: string): SystemBlock {
		return { index, text, cache_control: null, composition_match: null };
	}

	const BLOCK_0 = `You are Claude Code, Anthropic's official CLI for Claude, running within the Claude Agent SDK.
You are an interactive agent that helps users with software engineering tasks. Use the instructions below and the tools available to you to assist the user.

IMPORTANT: Assist with authorized security testing, defensive security, CTF challenges, and educational contexts. Refuse requests for destructive techniques, DoS attacks, mass targeting, supply chain compromise, or detection evasion for malicious purposes.

# Tone and style
 - Only use emojis if the user explicitly requests it. Avoid using emojis in all communication unless asked.
 - Your responses should be short and concise.
 - Do not use a colon before tool calls.`;

	const BLOCK_1 = `<system-reminder>
# Persistent Agent Memory

You have a persistent, file-based memory system at /Users/operator/.claude/agent-memory/. This directory already exists — write to it directly with the Write tool.
</system-reminder>`;

	const BLOCK_2 = `# Environment
You have been invoked in the following environment:
 - Primary working directory: /Users/operator/projects/ai/temet-proxy
 - Is a git repository: true
 - Platform: darwin
 - Shell: zsh`;

	const SOURCE_DETAIL = {
		id: SOURCE_REQUEST_ID,
		session_id: '01J9X4QF7H8K2N3P5R6T7V8W9Y-route-blitz',
		project: '/Users/operator/projects/ai/temet-proxy',
		human_prompt: 'Author the filter workbench route story.',
		system_blocks: {
			prompt_sha256: 'feeddeadbeef0123feeddeadbeef0123feeddeadbeef0123feeddeadbeef0123',
			composition_id: null,
			composition_version: null,
			blocks: [sysBlock(0, BLOCK_0), sysBlock(1, BLOCK_1), sysBlock(2, BLOCK_2)]
		}
	} as RequestDetail;

	function rule(overrides: {
		id: string;
		kind: FilterRuleDto['kind'];
		value: string;
		action: FilterRuleDto['action'];
		text: string | null;
		title?: string | null;
		sort_order: number;
	}): FilterRuleDto {
		return {
			id: overrides.id,
			kind: overrides.kind,
			value: overrides.value,
			action: overrides.action,
			text: overrides.text,
			title: overrides.title ?? null,
			enabled: true,
			source: 'ui',
			updated_at: '2026-06-10T09:00:00Z',
			trigger_token: null,
			sort_order: overrides.sort_order,
			fires_24h: 0,
			fires_7d: 0,
			last_fired_at: null
		};
	}

	const RULES: FilterRuleDto[] = [
		rule({
			id: 'wb-r1',
			kind: 'text_substring',
			value: 'write to it directly with the Write tool',
			action: 'replace',
			text: 'use the Write tool',
			sort_order: 1
		}),
		rule({
			id: 'wb-r2',
			kind: 'text_substring',
			value: 'Avoid using emojis in all communication unless asked.',
			action: 'drop',
			text: null,
			sort_order: 2
		}),
		rule({
			id: 'wb-r3',
			kind: 'text_substring',
			value:
				'Assist with authorized security testing, defensive security, CTF challenges, and educational contexts.',
			action: 'replace',
			text: '[SECURITY SECTION REDACTED BY FILTER]',
			sort_order: 3
		})
	];

	function buildMockRulesStore(rules: FilterRuleDto[]): FilterRulesStore {
		let scrollTarget: string | null = null;
		return {
			get rules() {
				return rules;
			},
			get loading() {
				return false;
			},
			get initialized() {
				return true;
			},
			get error() {
				return null;
			},
			get scrollTargetRuleId() {
				return scrollTarget;
			},
			set scrollTargetRuleId(value: string | null) {
				scrollTarget = value;
			},
			refresh: async () => {},
			create: async () => rules[0]!,
			update: async (id: string) => rules.find((r) => r.id === id) ?? rules[0]!,
			remove: async () => {},
			handleEvent: () => {}
		};
	}

	const ASSISTANT_HEALTH = {
		ok: true,
		enabled: true,
		model: 'claude-sonnet-5',
		effort: 'medium',
		may_apply: 'suggest',
		spent_today_usd: 0.08,
		daily_budget_usd: 1
	};

	function stubWorkbenchFetch(): void {
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/filter/rules/dry-run')) {
				return new Response(JSON.stringify({ error: 'dry-run endpoint not implemented' }), {
					status: 404,
					statusText: 'Not Found',
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/assistant/health')) {
				return new Response(JSON.stringify(ASSISTANT_HEALTH), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes(`/api/requests/${SOURCE_REQUEST_ID}`)) {
				return new Response(JSON.stringify(SOURCE_DETAIL), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Filter/Workbench',
		component: FilterWorkbenchRouteHarness,
		tags: ['autodocs'],
		argTypes: {
			rulesStore: { control: false }
		}
	});
</script>

<Story
	name="Default"
	args={{ rulesStore: buildMockRulesStore(RULES) }}
	loaders={[
		async () => {
			stubWorkbenchFetch();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/filter/workbench?from=request:${SOURCE_REQUEST_ID}`),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(await canvas.findByText('Filter workbench')).toBeInTheDocument();
		await expect(canvas.getByRole('tab', { name: 'As sent' })).toBeInTheDocument();
		await expect(canvas.getByRole('tab', { name: 'With my edits' })).toBeInTheDocument();
		await expect(canvas.getByRole('tab', { name: 'Final' })).toBeInTheDocument();

		// No rule ids anywhere in the edits list — plain titles only. The prompt pane's own
		// per-paragraph quick-edit marker can carry this same derived title text, so this is
		// found scoped to the edits sidebar section rather than via a bare text query that can
		// match either.
		await expect(canvas.queryByText('wb-r1')).toBeNull();
		await expect(canvas.queryByText('wb-r2')).toBeNull();
		const editsPanel = within(await canvas.findByRole('region', { name: 'Your edits' }));
		await expect(
			await editsPanel.findByText('Rewrites “write to it directly with the Write tool”')
		).toBeInTheDocument();

		// This 1200px harness is below the assistant column's 1280px open-by-default floor, so
		// it starts collapsed to its rail — expand it before checking the health-derived model
		// text inside it.
		await userEvent.click(await canvas.findByRole('button', { name: 'Expand Prompt assistant' }));
		await expect(await canvas.findByText(/claude-sonnet-5/)).toBeInTheDocument();

		const readOnly = await canvas.findByRole('button', { name: 'Read only' });
		await userEvent.click(readOnly);
		await expect(await canvas.findByRole('button', { name: 'Show edits' })).toBeInTheDocument();
		await userEvent.click(await canvas.findByRole('button', { name: 'Show edits' }));
		await expect(await canvas.findByRole('button', { name: 'Read only' })).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="h-[800px] w-[1200px] overflow-hidden bg-[var(--surface-bg)]">
			<FilterWorkbenchRouteHarness {...args} />
		</div>
	{/snippet}
</Story>
