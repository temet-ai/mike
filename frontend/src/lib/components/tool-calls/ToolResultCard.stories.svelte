<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within, userEvent } from 'storybook/test';
	import ToolResultCard from './ToolResultCard.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/ToolResultCard',
		parameters: {
			docs: {
				description: {
					component:
						'Renders a captured Claude tool_result block. Mirrors ToolCallCard card ' +
						'design language (same shell, same collapsible disclosure pattern). ' +
						'Shows a collapsed first-line preview with full content reachable on expand.'
				}
			}
		}
	});

	const bashResultContent = 'Tests passed: 142\nFailed: 0\nDuration: 3.4s\n\nAll tests passed.\n';
	const errorContent =
		"Error: ENOENT: no such file or directory, open '/nonexistent/path/to/file.rs'\n" +
		'    at Object.openSync (node:fs:601:3)\n' +
		'    at Object.readFileSync (node:fs:469:35)\n';
	const longResultContent =
		'Session list:\n' +
		Array.from({ length: 20 }, (_, i) => `  - session-${i + 1}: 42 requests`).join('\n') +
		'\n\nTotal: 20 sessions\n';
</script>

<Story
	name="Sequence"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const resultLabels = canvas.getAllByText(/^Result$/i);
		expect(resultLabels.length).toBeGreaterThan(0);

		expect(canvas.getByText('Error result')).toBeInTheDocument();
		expect(canvas.getByText('error')).toBeInTheDocument();

		expect(canvas.getAllByText('Bash').length).toBeGreaterThan(0);
		expect(canvas.getAllByText('Read').length).toBeGreaterThan(0);

		expect(canvas.getByText('Tests passed: 142')).toBeInTheDocument();

		const showButtons = canvas.getAllByRole('button', { name: /show output/i });
		expect(showButtons.length).toBeGreaterThan(0);
		await userEvent.click(showButtons[0]!);
		expect(canvas.getAllByText(/All tests passed\./).length).toBeGreaterThan(0);

		expect(canvas.getAllByRole('button', { name: /hide output/i }).length).toBeGreaterThan(0);
	}}
>
	<div
		style="display: flex; flex-direction: column; gap: 12px; max-width: 720px; padding: 16px; background: var(--surface-bg);"
	>
		<ToolResultCard toolName="Bash" content={bashResultContent} />
		<ToolResultCard toolName="Read" content={errorContent} isError={true} />
		<ToolResultCard content={longResultContent} />
		<ToolResultCard toolName="Bash" content={bashResultContent} rawOpen={true} />
	</div>
</Story>
