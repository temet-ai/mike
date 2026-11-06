<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import ReadCall from './ReadCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/ReadCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `Read` tool call: the full file path as the ' +
						'subject, with a line-range badge derived from offset/limit. Path renders ' +
						'verbatim and wraps — never truncated.'
				}
			}
		}
	});

	const readInput = {
		file_path: '/home/user/temet-proxy/crates/mike/src/store/persistence/messages.rs',
		offset: 120,
		limit: 60
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(
			canvas.getByText('/home/user/temet-proxy/crates/mike/src/store/persistence/messages.rs')
		).toBeInTheDocument();

		expect(canvas.getByText('lines 120–179')).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<ReadCall input={readInput} />
	</div>
</Story>
