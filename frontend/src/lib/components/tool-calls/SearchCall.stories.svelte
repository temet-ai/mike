<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import SearchCall from './SearchCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/SearchCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `Grep`/`Glob` tool call: the search pattern as ' +
						'the subject plus the scope it ran against (path, glob, type, output mode). ' +
						'Pattern and scope render verbatim and wrap — never truncated.'
				}
			}
		}
	});

	const searchInput = {
		pattern: 'fn (write_request_messages|persist_request)\\b',
		path: 'crates/mike/src/store/persistence',
		glob: '*.rs',
		type: 'rust',
		output_mode: 'content'
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(canvas.getByText('fn (write_request_messages|persist_request)\\b')).toBeInTheDocument();

		expect(canvas.getByText('crates/mike/src/store/persistence')).toBeInTheDocument();
		expect(canvas.getByText('*.rs')).toBeInTheDocument();
		expect(canvas.getByText('rust')).toBeInTheDocument();
		expect(canvas.getByText('content')).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<SearchCall input={searchInput} />
	</div>
</Story>
