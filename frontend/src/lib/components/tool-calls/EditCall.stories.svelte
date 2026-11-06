<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import EditCall from './EditCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/EditCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `Edit` tool call: the file path plus each ' +
						'old_string/new_string rendered as a verbatim −/+ before/after diff pair. ' +
						'Both sides render in full — never truncated.'
				}
			}
		}
	});

	const editInput = {
		file_path: '/home/user/temet-proxy/frontend/src/lib/api.ts',
		old_string: 'const PAGE_SIZE = 500;',
		new_string: 'const PAGE_SIZE = 1000;'
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(canvas.getByText('/home/user/temet-proxy/frontend/src/lib/api.ts')).toBeInTheDocument();

		expect(canvas.getByText('const PAGE_SIZE = 500;')).toBeInTheDocument();
		expect(canvas.getByText('const PAGE_SIZE = 1000;')).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<EditCall input={editInput} />
	</div>
</Story>
