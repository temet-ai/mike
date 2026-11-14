<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, within } from 'storybook/test';
	import { detailFixture, problemFixture } from '../__fixtures__/wright';
	import WrightProblemDetail from './WrightProblemDetail.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightProblemDetail',
		component: WrightProblemDetail,
		tags: ['autodocs']
	});

	const onUndo = fn();
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('work_thread_checkpoint is called without a tldr')).toBeVisible();
		await expect(canvas.getByText('Cause established')).toBeVisible();
		await expect(canvas.getByRole('list', { name: 'Repair 2 progress' })).toBeVisible();
		await expect(canvas.getByRole('link', { name: /Commit afc42e23c4d1/ })).toHaveAttribute(
			'href',
			'https://github.com/temet-ai/mike/commit/afc42e23c4d1'
		);
		await expect(canvas.getByText(/Run 3: Finished/)).toBeVisible();
		await expect(canvas.getByRole('button', { name: /^Undo: Close orphan launch/ })).toBeEnabled();
	}}
>
	{#snippet template()}
		<div
			class="edge"
			style="padding: 16px; max-inline-size: 44rem; --edge-surface: var(--surface-card);"
		>
			<WrightProblemDetail
				problem={problemFixture()}
				detail={detailFixture()}
				undo={{}}
				{onUndo}
				onMore={() => {}}
				onRetry={() => {}}
			/>
		</div>
	{/snippet}
</Story>
