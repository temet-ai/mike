<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ResponseBodyCard from './ResponseBodyCard.svelte';
	import { buildRequestDetail } from '$lib/components/__fixtures__/request';
	import { primeBodyCache } from '$lib/test-support/storyBodyCache';

	const ASSEMBLED_RESPONSE = JSON.stringify({
		id: 'msg_01StreamedExample',
		type: 'message',
		role: 'assistant',
		model: 'claude-opus-4-8',
		content: [
			{
				type: 'thinking',
				thinking:
					'The user is asking about a null-pointer issue. Let me trace the access on line 42 before proposing a fix.',
				signature: 'sig_streamed_example_abc123'
			},
			{
				type: 'text',
				text: 'Here is the analysis of your code. The main issue is on line 42 where the null check is missing. Consider adding `if (user === null) return;` before accessing user properties.'
			},
			{
				type: 'tool_use',
				id: 'toolu_01StreamedEdit',
				name: 'Edit',
				input: {
					file_path: '/src/user.ts',
					old_string: 'return user.name;',
					new_string: 'if (user === null) return;\n\treturn user.name;'
				}
			}
		]
	});

	const { Story } = defineMeta({
		title: 'Components/Detail/ResponseBodyCard',
		component: ResponseBodyCard,
		tags: ['autodocs']
	});
</script>

<Story
	name="SSE_Complete"
	loaders={[
		async () => {
			await primeBodyCache(303, 'response', ASSEMBLED_RESPONSE);
			return {};
		}
	]}
	args={{
		detail: buildRequestDetail({
			id: 303,
			is_streaming: true,
			status: 'Complete',
			accumulated_text: '',
			response_body_size: ASSEMBLED_RESPONSE.length,
			params: {
				max_tokens: 8096,
				temperature: null,
				thinking_enabled: true,
				thinking_budget: 4096,
				thinking_mode: 'enabled'
			},
			token_usage: {
				input_tokens: 1500,
				output_tokens: 55,
				cache_read_input_tokens: 0,
				cache_creation_input_tokens: 0
			},
			stop_reason: 'tool_use'
		})
	}}
>
	{#snippet template(args)}
		<ResponseBodyCard {...args} />
	{/snippet}
</Story>

<Story
	name="StreamingError"
	args={{
		detail: buildRequestDetail({
			id: 404,
			is_streaming: true,
			status: { Error: 'Connection reset by peer' },
			accumulated_text:
				'The analysis shows that the root cause is a null-pointer dereference on line 42. Here is the partial fix before the stream was interrupted by a connection error:\n\n```typescript\nif (user === null) return;\nreturn user.name;\n```',
			response_body_size: 0
		})
	}}
>
	{#snippet template(args)}
		<ResponseBodyCard {...args} />
	{/snippet}
</Story>
