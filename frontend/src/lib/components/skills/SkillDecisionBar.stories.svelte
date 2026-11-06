<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import SkillDecisionBar from './SkillDecisionBar.svelte';
	import { skillOutcome } from './skillOutcome';

	const onDecide = fn();

	const { Story } = defineMeta({
		title: 'Skills/SkillDecisionBar',
		component: SkillDecisionBar,
		tags: ['autodocs']
	});
</script>

<Story
	name="Waiting on you"
	args={{
		review: null,
		retired: false,
		editing: false,
		outcome: skillOutcome('ask', 'accepted', null, false),
		onDecide
	}}
	play={async ({ canvasElement }) => {
		onDecide.mockClear();
		const canvas = within(canvasElement);
		await expect(canvas.getByText(/only after you approve it/)).toBeInTheDocument();
		await expect(canvas.getByRole('button', { name: 'Undo decision' })).toBeDisabled();
		await expect(canvas.getByRole('button', { name: 'Restore' })).toBeDisabled();
		await userEvent.click(canvas.getByRole('button', { name: 'Approve' }));
		await expect(onDecide).toHaveBeenCalledWith('approve');
		await userEvent.click(canvas.getByRole('button', { name: 'Retire' }));
		await expect(onDecide).toHaveBeenCalledWith('retire');
	}}
>
	{#snippet template(args)}
		<div class="w-[48rem] bg-[var(--surface-bg)] p-4">
			<SkillDecisionBar {...args} />
		</div>
	{/snippet}
</Story>
