<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import SubjectsBand from './SubjectsBand.svelte';
	import { SUBJECTS } from '$lib/test-support/__fixtures__/knowledgeConsolidation';

	const { Story } = defineMeta({
		title: 'Components/Memory/SubjectsBand',
		component: SubjectsBand,
		tags: ['autodocs']
	});
</script>

<Story
	name="Subjects"
	play={async ({ canvas }) => {
		await expect(canvas.getByRole('table', { name: 'Subjects' })).toBeVisible();
		await expect(canvas.getByRole('link', { name: 'Deploy order' })).toHaveAttribute(
			'href',
			'/memory/subjects/61?project=3'
		);
		await expect(canvas.getAllByText('waiting').length).toBe(2);
		await expect(canvas.getByText(/^Conflict ·/)).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="edge w-[960px]">
			<SubjectsBand subjects={SUBJECTS} project="3" />
		</div>
	{/snippet}
</Story>
