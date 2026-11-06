<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import BodyViewer from './BodyViewer.svelte';
	import type { BodyPayload } from '$lib/types';

	function buildPayload(text: string | null, extra?: Partial<BodyPayload>): BodyPayload {
		return {
			text,
			byte_size: text === null ? 0 : new TextEncoder().encode(text).length,
			has_body: text !== null,
			...extra
		};
	}

	const WITH_TOOL_USE_PAYLOAD = buildPayload(
		JSON.stringify(
			{
				model: 'claude-opus-4-7',
				max_tokens: 4096,
				messages: [
					{ role: 'user', content: 'What files are in the current directory?' },
					{
						role: 'assistant',
						content: [
							{ type: 'text', text: 'I will list the files for you.' },
							{
								type: 'tool_use',
								id: 'toolu_01ABC',
								name: 'Bash',
								input: { command: 'ls -la' }
							}
						]
					},
					{
						role: 'user',
						content: [
							{
								type: 'tool_result',
								tool_use_id: 'toolu_01ABC',
								content:
									'total 8\ndrwxr-xr-x  4 user staff  128 May  4 12:00 .\ndrwxr-xr-x 12 user staff  384 May  4 11:00 ..\n-rw-r--r--  1 user staff 1234 May  4 10:00 README.md\n-rw-r--r--  1 user staff  567 May  4 09:00 package.json'
							}
						]
					}
				]
			},
			null,
			2
		)
	);

	const { Story } = defineMeta({
		title: 'Components/Detail/BodyViewer',
		component: BodyViewer,
		tags: ['autodocs'],
		argTypes: {
			label: {
				control: 'select',
				options: ['request', 'response']
			}
		}
	});
</script>

<Story name="WithToolUse" args={{ payload: WITH_TOOL_USE_PAYLOAD, label: 'request' }}>
	{#snippet template(args)}
		<div class="p-4 max-w-[700px]">
			<BodyViewer {...args} />
		</div>
	{/snippet}
</Story>
