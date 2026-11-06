<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import IdentifiersSection from './IdentifiersSection.svelte';
	import type { RequestDetail } from '$lib/types';

	function makeDetail(overrides: Partial<RequestDetail> = {}): RequestDetail {
		return {
			id: 42,
			upstream_request_id: 'req_01ABCdefGhij',
			upstream_message_id: 'msg_01ABCdefGhij',
			session_id: 'f47ac10b-58cc-4372-a567-0e02b2c3d479',
			project: 'proxy',
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
			token_usage: null,
			stop_reason: null,
			created_at: '2026-04-17T12:00:00.000Z',
			completed_at: '2026-04-17T12:00:05.000Z',
			...overrides
		};
	}

	const { Story } = defineMeta({
		title: 'Components/Detail/IdentifiersSection',
		component: IdentifiersSection,
		tags: ['autodocs']
	});
</script>

<Story
	name="WithPromptShaLink"
	args={{
		detail: makeDetail({
			system_blocks: {
				prompt_sha256: '9f2c4e6a8b0d1f3e5a7c9b1d3f5e7a9c0b2d4f6e8a0c2e4f6a8b0d2f4e6a8c0d',
				composition_id: null,
				composition_version: null,
				blocks: []
			}
		})
	}}
>
	{#snippet template(args)}
		<IdentifiersSection {...args} />
	{/snippet}
</Story>
