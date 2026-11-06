<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MessageContentRenderer from './MessageContentRenderer.svelte';
	import type { RenderedBlock } from './MessageContentRenderer.svelte';

	const MIXED_BLOCKS: RenderedBlock[] = [
		{ kind: 'text', text: 'Let me check the directory contents.' },
		{
			kind: 'tool_use',
			id: 'toolu_bash_01',
			name: 'Bash',
			input: {
				command: 'ls -la /Users/operator/projects/ai/temet-proxy/frontend/src/lib/components/'
			}
		},
		{
			kind: 'tool_result',
			tool_use_id: 'toolu_bash_01',
			blocks: [
				{
					kind: 'text',
					text: 'total 320\ndrwxr-xr-x  42 user staff  1344 May  4 12:00 .\ndrwxr-xr-x  11 user staff   352 May  4 11:00 ..\n-rw-r--r--   1 user staff  4821 May  4 10:00 BodyViewer.svelte\n-rw-r--r--   1 user staff  7234 May  4 10:00 HeadersTable.svelte\n-rw-r--r--   1 user staff 12048 May  4 10:00 SystemPromptBreakdown.svelte\n-rw-r--r--   1 user staff  3512 May  4 10:00 ToolNamesChips.svelte'
				}
			]
		},
		{ kind: 'text', text: 'The components directory contains 42 files.' }
	];

	const { Story } = defineMeta({
		title: 'Components/MessageView/MessageContentRenderer',
		component: MessageContentRenderer,
		tags: ['autodocs'],
		argTypes: {
			blocks: { control: 'object' }
		}
	});
</script>

<Story name="Mixed" args={{ blocks: MIXED_BLOCKS }}>
	{#snippet template(args)}
		<div class="p-4 max-w-[600px]">
			<MessageContentRenderer {...args} />
		</div>
	{/snippet}
</Story>
