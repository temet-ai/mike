<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import SkillRevisionList from './SkillRevisionList.svelte';

	const REVISIONS = [
		{
			id: 52,
			revision: 2,
			lesson_watermark: 812,
			status: 'accepted' as const,
			review: 'approved' as const,
			created_at: '2026-10-14T12:00:00Z'
		},
		{
			id: 41,
			revision: 1,
			lesson_watermark: 812,
			status: 'rejected' as const,
			review: null,
			created_at: '2026-10-14T09:12:00Z'
		}
	];

	const { Story } = defineMeta({
		title: 'Skills/SkillRevisionList',
		component: SkillRevisionList,
		tags: ['autodocs']
	});
</script>

<Story
	name="Two revisions"
	args={{ revisions: REVISIONS, current: 41 }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('link', { name: /Revision 2/ })).toHaveAttribute('href', '/skills/52');
		await expect(canvas.getByText(/passed review, approved by you/)).toBeInTheDocument();
		await expect(canvas.getByText(/did not pass review/).closest('li')).toHaveAttribute(
			'aria-current',
			'page'
		);
	}}
>
	{#snippet template(args)}
		<div class="w-[32rem] bg-[var(--surface-bg)] p-4">
			<SkillRevisionList {...args} />
		</div>
	{/snippet}
</Story>
