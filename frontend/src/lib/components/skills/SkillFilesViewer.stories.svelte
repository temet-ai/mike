<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import SkillFilesViewer from './SkillFilesViewer.svelte';
	import { SKILL_DRAFT } from '$lib/test-support/skillsFixtures';

	let editing = $state(false);
	let text = $state(SKILL_DRAFT.skill_md);

	const { Story } = defineMeta({
		title: 'Skills/SkillFilesViewer',
		component: SkillFilesViewer,
		tags: ['autodocs']
	});
</script>

<Story
	name="Read, switch files and edit"
	play={async ({ canvasElement }) => {
		editing = false;
		const canvas = within(canvasElement);
		await expect(canvas.getByText(/pnpm test checkout\/retry/)).toBeInTheDocument();
		await userEvent.click(canvas.getByRole('tab', { name: 'references/copy-locations.md' }));
		await expect(
			canvas.getByText(/Claude reads this when you change text a customer sees/)
		).toBeInTheDocument();
		await userEvent.click(canvas.getByRole('tab', { name: 'SKILL.md' }));
		await userEvent.click(canvas.getByRole('button', { name: 'Edit' }));
		const field = canvas.getByRole('textbox', { name: /SKILL.md/ });
		await userEvent.type(field, 'Extra line.');
		await expect(canvas.getByTestId('text')).toHaveTextContent('edited: true');
	}}
>
	{#snippet template()}
		<div class="w-[48rem] bg-[var(--surface-bg)] p-4">
			<button type="button" onclick={() => (editing = true)}>Edit</button>
			<SkillFilesViewer
				skillMd={SKILL_DRAFT.skill_md}
				references={SKILL_DRAFT.reference_files}
				{editing}
				bind:draftText={text}
			/>
			<p data-testid="text" hidden>edited: {text.endsWith('Extra line.')}</p>
		</div>
	{/snippet}
</Story>
