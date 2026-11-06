<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MetricsStrip from './MetricsStrip.svelte';
	import type { RequestDetail, TokenUsage } from '$lib/types';

	function buildDetail(overrides: {
		token_usage: TokenUsage | null;
		stop_reason: string | null;
	}): RequestDetail {
		return {
			id: 1,
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
			model: 'claude-opus-4-6',
			is_streaming: false,
			message_count: 0,
			is_tool_loop: false,
			last_user_message: '',
			tool_names: [],
			params: {
				max_tokens: null,
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
			token_usage: overrides.token_usage,
			stop_reason: overrides.stop_reason,
			created_at: '2026-04-17T00:00:00Z',
			completed_at: '2026-04-17T00:00:30Z',
			diagnostics: []
		};
	}

	const FULL_USAGE = buildDetail({
		token_usage: {
			input_tokens: 4000,
			output_tokens: 3847,
			cache_read_input_tokens: 38200,
			cache_creation_input_tokens: 1204
		},
		stop_reason: 'end_turn'
	});

	const { Story } = defineMeta({
		title: 'Components/Detail/MetricsStrip',
		component: MetricsStrip,
		tags: ['autodocs']
	});
</script>

<Story name="FullUsage" args={{ detail: FULL_USAGE }}>
	{#snippet template(args)}
		<MetricsStrip {...args} />
	{/snippet}
</Story>
