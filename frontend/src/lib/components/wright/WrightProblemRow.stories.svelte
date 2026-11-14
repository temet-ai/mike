<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import { healthyProblems, problemFixture } from '../__fixtures__/wright';
	import WrightProblemRow from './WrightProblemRow.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightProblemRow',
		component: WrightProblemRow,
		tags: ['autodocs']
	});

	const onOpen = fn();
	const OPEN = problemFixture();
	const HEALTHY = healthyProblems()[0]!;
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Open')).toBeVisible();
		await expect(canvas.getByText('Healthy')).toBeVisible();
		await expect(canvas.getAllByText(/failures 3/)).toHaveLength(2);
		await expect(canvas.getByRole('link', { name: /Request 799713/ })).toHaveAttribute(
			'href',
			'/api/requests/799713'
		);
		await expect(
			canvas.getByRole('link', { name: /Tool errors for mcp__tether__work_thread_checkpoint/ })
		).toHaveAttribute('href', '/tools?tool_name=mcp__tether__work_thread_checkpoint');
		const toggles = canvas.getAllByRole('button');
		const first = toggles[0]!;
		await expect(first).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(first);
		await expect(first).toHaveAttribute('aria-expanded', 'true');
		await expect(canvas.getByText('Detail is here.')).toBeVisible();
		await expect(onOpen).toHaveBeenCalledWith(OPEN.id);
	}}
>
	{#snippet template()}
		<ul
			class="edge"
			style="list-style: none; margin: 0; padding: 0; --edge-surface: var(--surface-card);"
		>
			<WrightProblemRow problem={OPEN} {onOpen}>
				<p>Detail is here.</p>
			</WrightProblemRow>
			<WrightProblemRow problem={HEALTHY} />
		</ul>
	{/snippet}
</Story>
