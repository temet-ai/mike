<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import BashCall from './BashCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/BashCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `Bash` tool call: the full command on a ' +
						'`$`-prompted terminal row, with an optional description caption. The ' +
						'command renders verbatim and wraps — never truncated.'
				}
			}
		}
	});

	const bashInput = {
		command: "cargo nextest run -p mike --test proxy_roundtrip -E 'test(streams_sse_passthrough)'",
		description: 'Run the SSE passthrough integration test'
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(
			canvas.getByText(
				"cargo nextest run -p mike --test proxy_roundtrip -E 'test(streams_sse_passthrough)'"
			)
		).toBeInTheDocument();

		expect(canvas.getByText('Run the SSE passthrough integration test')).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<BashCall input={bashInput} />
	</div>
</Story>
