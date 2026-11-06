<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import SkillLessonList from './SkillLessonList.svelte';
	import { SKILL_LESSONS } from '$lib/test-support/skillsFixtures';

	const { Story } = defineMeta({
		title: 'Skills/SkillLessonList',
		component: SkillLessonList,
		tags: ['autodocs']
	});
</script>

<Story
	name="Lessons behind a skill"
	args={{ lessons: SKILL_LESSONS }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('link', { name: 'Lesson 801' })).toHaveAttribute(
			'href',
			'/sessions/improvement/lessons/801'
		);
		await expect(canvas.getByText(/Learned from: you changed the total again/)).toBeInTheDocument();
		await expect(canvas.getByText(/locales\/en\/checkout.json/)).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="w-[32rem] bg-[var(--surface-bg)] p-4">
			<SkillLessonList {...args} />
		</div>
	{/snippet}
</Story>
