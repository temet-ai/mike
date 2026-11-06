<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import SkillPlacementAdvice from './SkillPlacementAdvice.svelte';
	import { FOLDER_COVERED, SKILL_LESSONS } from '$lib/test-support/skillsFixtures';

	const onDecide = fn();

	const { Story } = defineMeta({
		title: 'Skills/SkillPlacementAdvice',
		component: SkillPlacementAdvice,
		tags: ['autodocs']
	});
</script>

<Story
	name="Existing skill covers the lessons"
	args={{
		review: FOLDER_COVERED.reviewer!,
		variant: 'page',
		draftReview: null,
		retired: false,
		editing: false,
		busy: null,
		existingHref: '/skills/17',
		lessons: SKILL_LESSONS,
		onDecide
	}}
	play={async ({ canvasElement }) => {
		onDecide.mockClear();
		const canvas = within(canvasElement);
		await expect(
			canvas.getByText(
				'The reviewer advised against a new skill, because the skill invoice-rounding already covers these lessons.'
			)
		).toBeInTheDocument();
		await expect(
			canvas.getByText(/The rounding steps repeat what invoice-rounding already tells Claude/)
		).toBeInTheDocument();
		await expect(
			canvas.getByText('mike does not yet move these lessons into invoice-rounding by itself.')
		).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: /Open invoice-rounding/ })).toHaveAttribute(
			'href',
			'/skills/17'
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Ship it anyway' }));
		await expect(onDecide).toHaveBeenCalledWith('approve');
		await userEvent.click(canvas.getByRole('button', { name: 'Do not ship it' }));
		await expect(onDecide).toHaveBeenCalledWith('reject');
	}}
>
	{#snippet template(args)}
		<div class="w-[48rem] bg-[var(--surface-bg)] p-4">
			<SkillPlacementAdvice {...args} />
		</div>
	{/snippet}
</Story>
