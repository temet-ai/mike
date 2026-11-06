<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import ImprovementStrip from './ImprovementStrip.svelte';
	import { IMPROVEMENT_SUMMARY, TREND_RESPONSE } from '$lib/test-support/improvementFixtures';

	const { Story } = defineMeta({
		title: 'Improvement/ImprovementStrip',
		component: ImprovementStrip,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story
	name="Default"
	args={{
		summary: { ...IMPROVEMENT_SUMMARY, openProposalCount: 0 },
		summaryError: null,
		latest: TREND_RESPONSE.weekly.at(-1) ?? null,
		onPick: fn()
	}}
	play={async ({ args, canvas }) => {
		const lessons = canvas.getByRole('button', { name: /^Lessons: 4\./ });
		await expect(lessons).toHaveTextContent('4');
		await expect(
			canvas.getByRole('button', { name: /^Nudges to answer: 2\./ })
		).toBeInTheDocument();
		await expect(canvas.getByRole('button', { name: /^Directives: 0\./ })).toBeInTheDocument();

		await expect(canvas.getByText('Lessons approved')).toBeInTheDocument();
		await expect(canvas.getByText('29')).toBeInTheDocument();
		await expect(canvas.getByText('10 rejected')).toBeInTheDocument();
		await expect(canvas.getByText('7 wakes · 3 notes')).toBeInTheDocument();
		await expect(canvas.getByText('25%')).toBeInTheDocument();
		await expect(canvas.getByText(/of 283 judged turns/)).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: 'See the trend' })).toHaveAttribute(
			'href',
			'#imp-better'
		);

		const left = lessons.getBoundingClientRect().left;
		await userEvent.hover(lessons);
		await expect(lessons.getBoundingClientRect().left).toBe(left);
		await userEvent.click(canvas.getByRole('button', { name: /^Nudges to answer/ }));
		await expect(args.onPick).toHaveBeenCalledWith('nudges');
	}}
>
	{#snippet template(args)}
		<div class="bg-[var(--surface-bg)] px-6 py-4 [container-type:inline-size]">
			<ImprovementStrip {...args} />
		</div>
	{/snippet}
</Story>
