<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import FetchCall from './FetchCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/FetchCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `WebFetch` tool call: the URL as a clickable ' +
						'link affordance plus the full prompt the model asked of the fetched page. ' +
						'URL and prompt render verbatim and wrap — never truncated.'
				}
			}
		}
	});

	const fetchInput = {
		url: 'https://docs.anthropic.com/en/api/messages-streaming',
		prompt: 'Extract the exact ordering of SSE event types for a streaming /v1/messages response.'
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const link = canvas.getByRole('link', {
			name: 'https://docs.anthropic.com/en/api/messages-streaming'
		});
		expect(link).toHaveAttribute('href', 'https://docs.anthropic.com/en/api/messages-streaming');

		expect(
			canvas.getByText(
				'Extract the exact ordering of SSE event types for a streaming /v1/messages response.'
			)
		).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<FetchCall input={fetchInput} />
	</div>
</Story>
