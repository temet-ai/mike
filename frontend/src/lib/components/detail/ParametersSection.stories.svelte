<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ParametersSection from './ParametersSection.svelte';
	import type { RequestDetail } from '$lib/types';

	function makeDetail(overrides: Partial<RequestDetail> = {}): RequestDetail {
		return {
			id: 42,
			upstream_request_id: null,
			upstream_message_id: null,
			session_id: 'sess-stories',
			project: null,
			human_prompt: '',
			turn_type: 'user_prompt',
			method: 'POST',
			path: '/v1/messages',
			request_headers: [],
			request_body_size: 0,
			model: null,
			is_streaming: true,
			message_count: 1,
			is_tool_loop: false,
			last_user_message: '',
			tool_names: [],
			params: {
				max_tokens: 64000,
				temperature: 1.0,
				thinking_enabled: true,
				thinking_budget: 16384,
				thinking_mode: 'enabled',
				effort: 'xhigh'
			},
			status: 'Complete',
			response_status: 200,
			response_headers: null,
			response_body_size: 0,
			accumulated_text: '',
			accumulated_thinking: '',
			tool_calls: [],
			token_usage: null,
			stop_reason: null,
			created_at: '2026-04-17T12:00:00.000Z',
			completed_at: '2026-04-17T12:00:05.000Z',
			...overrides
		};
	}

	const { Story } = defineMeta({
		title: 'Components/Detail/ParametersSection',
		component: ParametersSection,
		tags: ['autodocs']
	});
</script>

<Story name="Default" args={{ detail: makeDetail() }}>
	{#snippet template(args)}
		<ParametersSection {...args} />
	{/snippet}
</Story>
