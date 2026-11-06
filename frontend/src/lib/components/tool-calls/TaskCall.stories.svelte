<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import TaskCall from './TaskCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/TaskCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `Task`/`Agent` tool call: the subagent type and ' +
						'short description plus the full prompt the subagent receives. The prompt ' +
						'renders verbatim and wraps — never truncated.'
				}
			}
		}
	});

	const taskInput = {
		subagent_type: 'frontend-developer',
		description: 'Add tool-call stories',
		prompt:
			'Add one *.stories.svelte file per tool-call component under ' +
			'frontend/src/lib/components/tool-calls/, each with exactly one representative ' +
			'<Story> rendering the component with realistic real-shaped input. Verify with ' +
			'scripts/sb-test and pnpm run check.'
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(canvas.getByText('frontend-developer')).toBeInTheDocument();
		expect(canvas.getByText('Add tool-call stories')).toBeInTheDocument();

		expect(
			canvas.getByText(/Add one \*\.stories\.svelte file per tool-call component/)
		).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<TaskCall input={taskInput} />
	</div>
</Story>
