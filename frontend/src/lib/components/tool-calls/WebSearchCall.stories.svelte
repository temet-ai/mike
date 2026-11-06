<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import WebSearchCall from './WebSearchCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/WebSearchCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `WebSearch` tool call: the query as the subject ' +
						'plus the allowed (+) and blocked (−) domain scope. Query and domains render ' +
						'verbatim and wrap — never truncated.'
				}
			}
		}
	});

	const webSearchInput = {
		query: 'axum 0.7 SSE keep-alive interval streaming response',
		allowed_domains: ['docs.rs', 'github.com'],
		blocked_domains: ['pinterest.com']
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(
			canvas.getByText('axum 0.7 SSE keep-alive interval streaming response')
		).toBeInTheDocument();

		expect(canvas.getByText('+ docs.rs')).toBeInTheDocument();
		expect(canvas.getByText('+ github.com')).toBeInTheDocument();
		expect(canvas.getByText('− pinterest.com')).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<WebSearchCall input={webSearchInput} />
	</div>
</Story>
