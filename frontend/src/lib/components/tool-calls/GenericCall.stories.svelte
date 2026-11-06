<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import GenericCall from './GenericCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/GenericCall',
		parameters: {
			docs: {
				description: {
					component:
						'Fallback per-kind body for a captured tool call with no bespoke chrome: ' +
						'every top-level input field rendered as a key/value row. Values render ' +
						'verbatim and wrap — never truncated.'
				}
			}
		}
	});

	const genericInput = {
		operation: 'reindex',
		dry_run: true,
		targets: ['sessions', 'requests'],
		batch_size: 500
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(canvas.getByText('operation')).toBeInTheDocument();
		expect(canvas.getByText('reindex')).toBeInTheDocument();
		expect(canvas.getByText('dry_run')).toBeInTheDocument();
		expect(canvas.getByText('true')).toBeInTheDocument();
		expect(canvas.getByText('targets')).toBeInTheDocument();
		expect(canvas.getByText('["sessions","requests"]')).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<GenericCall input={genericInput} />
	</div>
</Story>
