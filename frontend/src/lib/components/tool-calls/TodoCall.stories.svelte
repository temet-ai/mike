<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import TodoCall from './TodoCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/TodoCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `TodoWrite` tool call: the `todos` array as a ' +
						'checklist with a per-item status marker (✓ completed / ◐ in_progress / ○ ' +
						'pending). Every item renders in full — no slicing.'
				}
			}
		}
	});

	const todoInput = {
		todos: [
			{ content: 'Read the per-kind tool-call components', status: 'completed' },
			{ content: 'Author one representative story per component', status: 'in_progress' },
			{ content: 'Run scripts/sb-test and pnpm run check', status: 'pending' }
		]
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(canvas.getByText('3 items')).toBeInTheDocument();
		expect(canvas.getByText('Read the per-kind tool-call components')).toBeInTheDocument();
		expect(canvas.getByText('Author one representative story per component')).toBeInTheDocument();
		expect(canvas.getByText('Run scripts/sb-test and pnpm run check')).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<TodoCall input={todoInput} />
	</div>
</Story>
