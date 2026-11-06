<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, fn } from 'storybook/test';
	import TimelineCommandBar from './TimelineCommandBar.svelte';

	const { Story } = defineMeta({
		title: 'Shell/TimelineCommandBar',
		component: TimelineCommandBar,
		tags: ['autodocs'],
		args: {
			toolOptions: [
				{ value: 'Read', label: 'Read (14)' },
				{ value: 'Edit', label: 'Edit (6)' },
				{ value: 'Bash', label: 'Bash (9)' },
				{ value: 'Task', label: 'Task (2)' }
			],
			errorTurnCount: 2,
			dispatchTurnCount: 1,
			requestCount: 214,
			turnCount: 18,
			isLive: true,
			onJump: fn((n: number) => (n > 214 ? `#${n} not in range` : null)),
			onExpandAll: fn(),
			onCollapseAll: fn(),
			operatorOnly: true,
			onToggleOperatorOnly: fn()
		}
	});
</script>

<Story
	name="Default"
	args={{ query: '', role: 'all', tool: 'all' }}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);

		const search = canvas.getByPlaceholderText(/Search prompts/i);
		await userEvent.type(search, 'reasoning');
		await expect(search).toHaveValue('reasoning');
		const clear = canvas.getByLabelText('Clear search');
		await userEvent.click(clear);
		await expect(search).toHaveValue('');

		const jumpInput = canvas.getByPlaceholderText('req');
		await userEvent.type(jumpInput, '42');
		await userEvent.keyboard('{Enter}');
		await expect(args.onJump).toHaveBeenCalledWith(42);

		await userEvent.click(canvas.getByRole('button', { name: /expand all/i }));
		await expect(args.onExpandAll).toHaveBeenCalled();
		await userEvent.click(canvas.getByRole('button', { name: /collapse all/i }));
		await expect(args.onCollapseAll).toHaveBeenCalled();

		await userEvent.click(canvas.getByRole('button', { name: /Errors/i }));
		await expect(canvas.getByRole('button', { name: /Errors/i })).toHaveAttribute(
			'aria-pressed',
			'true'
		);

		const toolsBtn = canvas.getByRole('button', { name: /^Tools/i });
		await expect(toolsBtn).toHaveAttribute('aria-haspopup', 'menu');
		await userEvent.click(toolsBtn);
		await expect(toolsBtn).toHaveAttribute('aria-expanded', 'true');
		await userEvent.click(canvas.getByRole('menuitemradio', { name: /Bash/i }));
		await expect(canvas.getByRole('button', { name: /^Tools/i })).toHaveTextContent(/Tools: Bash/i);

		const operatorOnlyBtn = canvas.getByRole('button', { name: /Operator only/i });
		await expect(operatorOnlyBtn).toHaveAttribute('aria-pressed', 'true');
		await userEvent.click(operatorOnlyBtn);
		await expect(args.onToggleOperatorOnly).toHaveBeenCalled();
	}}
>
	{#snippet template(args)}
		<div style="width: 1100px; background: var(--surface-bg);">
			<TimelineCommandBar {...args} />
		</div>
	{/snippet}
</Story>
