<!--
What is waiting on you, as three tabs with counts over the real Lessons, Nudges and Directives
panels. The tabs stay rendered when a count is 0; that panel says so itself.
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import WaitingForYou from './WaitingForYou.svelte';
	import type { WaitingTab } from './WaitingForYou.svelte';
	import LessonCandidateList from './LessonCandidateList.svelte';
	import GuardWakeList from './GuardWakeList.svelte';
	import DirectiveProposalsSection from '$lib/components/memory/DirectiveProposalsSection.svelte';
	import { GUARD_WAKES, LESSON_CANDIDATES } from '$lib/test-support/improvementFixtures';

	const { Story } = defineMeta({
		title: 'Improvement/WaitingForYou',
		component: WaitingForYou,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let tab = $state<WaitingTab>('lessons');
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const lessonsTab = canvas.getByRole('tab', { name: /Lessons/ });
		const nudgesTab = canvas.getByRole('tab', { name: /Nudges/ });
		const directivesTab = canvas.getByRole('tab', { name: /Directives/ });
		await expect(lessonsTab).toHaveTextContent('Lessons4');
		await expect(nudgesTab).toHaveTextContent('Nudges2');
		await expect(directivesTab).toHaveTextContent('Directives0');
		await expect(lessonsTab).toHaveAttribute('aria-selected', 'true');
		await expect(canvas.getByText('Leave the code better than you found it.')).toBeVisible();

		await userEvent.click(nudgesTab);
		await expect(nudgesTab).toHaveAttribute('aria-selected', 'true');
		await expect(canvas.getByTestId('guard-marks-effect')).toBeVisible();
		await expect(canvas.getByText('Not ready to answer yet')).toBeVisible();

		await userEvent.click(directivesTab);
		await expect(canvas.getByText('No standing directives are waiting on you.')).toBeVisible();
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<WaitingForYou
				counts={{ lessons: LESSON_CANDIDATES.length, nudges: 2, directives: 0 }}
				bind:tab
			>
				{#snippet lessons()}
					<LessonCandidateList
						candidates={LESSON_CANDIDATES}
						error={null}
						detailHrefFor={(id) => `/sessions/improvement/lessons/${id}`}
						onApprove={async () => {}}
						onReject={async () => {}}
						onRetry={() => {}}
					/>
				{/snippet}
				{#snippet nudges()}
					<GuardWakeList
						wakes={GUARD_WAKES}
						error={null}
						emptyText="No nudges are waiting for your answer."
						onLabel={async () => {}}
						onRetry={() => {}}
					/>
				{/snippet}
				{#snippet directives()}
					<DirectiveProposalsSection
						proposals={[]}
						error={null}
						emptyText="No standing directives are waiting on you."
					/>
				{/snippet}
			</WaitingForYou>
		</div>
	{/snippet}
</Story>
