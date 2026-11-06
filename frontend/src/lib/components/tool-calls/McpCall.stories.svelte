<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import McpCall from './McpCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/McpCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `mcp__<server>__<method>` tool call: the ' +
						'server·method split plus the input arguments as a key/value list. The full ' +
						'name and every argument value render verbatim — never truncated.'
				}
			}
		}
	});

	const mcpName = 'mcp__mike__list_requests';
	const mcpInput = {
		session_id: '7f3a9c2e-1b4d-4e8a-9c0f-2a1b3c4d5e6f',
		limit: 50,
		order: 'asc'
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(canvas.getByText('mike')).toBeInTheDocument();
		expect(canvas.getByText('list_requests')).toBeInTheDocument();

		expect(canvas.getByText('session_id')).toBeInTheDocument();
		expect(canvas.getByText('7f3a9c2e-1b4d-4e8a-9c0f-2a1b3c4d5e6f')).toBeInTheDocument();
		expect(canvas.getByText('order')).toBeInTheDocument();
		expect(canvas.getByText('asc')).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<McpCall name={mcpName} input={mcpInput} />
	</div>
</Story>
