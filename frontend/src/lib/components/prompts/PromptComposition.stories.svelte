<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import PromptComposition from './PromptComposition.svelte';
	import type { RequestDetail, SystemBlock, SystemBlocksProjection } from '$lib/types';

	interface Fixture {
		id: number;
		system_blocks: SystemBlocksProjection | null;
		input_tokens: number | null;
		max_tokens: number | null;
	}

	function buildDetail(f: Fixture): RequestDetail {
		return {
			id: f.id,
			upstream_request_id: null,
			upstream_message_id: null,
			session_id: 'storybook-session',
			project: null,
			human_prompt: '',
			turn_type: 'user_prompt',
			method: 'POST',
			path: '/v1/messages',
			request_headers: [],
			request_body_size: 0,
			model: 'claude-opus-4-7',
			is_streaming: false,
			message_count: 0,
			is_tool_loop: false,
			last_user_message: '',
			tool_names: [],
			params: {
				max_tokens: f.max_tokens,
				temperature: null,
				thinking_enabled: false,
				thinking_budget: null,
				thinking_mode: null
			},
			status: 'Complete',
			response_status: 200,
			response_headers: null,
			response_body_size: 0,
			accumulated_text: '',
			accumulated_thinking: '',
			tool_calls: [],
			token_usage:
				f.input_tokens === null
					? null
					: {
							input_tokens: f.input_tokens,
							output_tokens: 0,
							cache_read_input_tokens: 0,
							cache_creation_input_tokens: 0
						},
			stop_reason: null,
			created_at: '2026-04-17T00:00:00Z',
			completed_at: '2026-04-17T00:00:30Z',
			diagnostics: [],
			system_blocks: f.system_blocks ?? undefined
		};
	}

	function makeBlocks(texts: string[]): SystemBlock[] {
		return texts.map((text, index): SystemBlock => ({
			index,
			text,
			cache_control: null,
			composition_match: null
		}));
	}

	const FULL_SYSTEM_BLOCKS: SystemBlocksProjection = {
		prompt_sha256: null,
		composition_id: null,
		composition_version: null,
		blocks: makeBlocks([
			'You are Claude, an AI assistant built by Anthropic. You operate inside a reverse proxy that captures every /v1/messages request for later inspection.',
			'Environment: macOS 25.4.0. Shell: zsh. Working directory: /Users/dev/projects/mike. Today is 2026-04-17.',
			'Tool-use conventions: prefer the specialized Glob / Grep / Read tools over shelling out to find / grep / cat. Batch independent tool calls into a single response when possible.',
			'When editing, always prefer editing existing files rather than creating new ones. Never create documentation files unless the user explicitly asks.'
		])
	};

	const FULL_DETAIL: RequestDetail = buildDetail({
		id: 201,
		system_blocks: FULL_SYSTEM_BLOCKS,
		input_tokens: 18_400,
		max_tokens: 64_000
	});

	const MINIMAL_DETAIL: RequestDetail = buildDetail({
		id: 202,
		system_blocks: null,
		input_tokens: null,
		max_tokens: null
	});

	const WARNING_DETAIL: RequestDetail = buildDetail({
		id: 203,
		system_blocks: {
			prompt_sha256: null,
			composition_id: null,
			composition_version: null,
			blocks: makeBlocks(['You are Claude. Follow the user request. Keep responses concise.'])
		},
		input_tokens: 60_200,
		max_tokens: 64_000
	});

	const { Story } = defineMeta({
		title: 'Routes/Prompts/PromptComposition',
		component: PromptComposition,
		tags: ['autodocs']
	});
</script>

<Story name="FullComposition" args={{ detail: FULL_DETAIL }}>
	{#snippet template(args)}
		<div class="flex flex-col gap-4 p-4" style="background:var(--surface-card)">
			<PromptComposition {...args} />
		</div>
	{/snippet}
</Story>

<Story name="MinimalComposition" args={{ detail: MINIMAL_DETAIL }}>
	{#snippet template(args)}
		<div class="flex flex-col gap-4 p-4" style="background:var(--surface-card)">
			<PromptComposition {...args} />
		</div>
	{/snippet}
</Story>

<Story name="BudgetWarning" args={{ detail: WARNING_DETAIL }}>
	{#snippet template(args)}
		<div class="flex flex-col gap-4 p-4" style="background:var(--surface-card)">
			<PromptComposition {...args} />
		</div>
	{/snippet}
</Story>
