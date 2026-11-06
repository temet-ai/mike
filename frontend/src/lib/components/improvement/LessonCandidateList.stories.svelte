<!--
The Lessons panel: the lead line, then one group per project (alphabetical, "No project" last),
newest first inside each. A decided lesson stays in its place.
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import { SvelteMap } from 'svelte/reactivity';
	import LessonCandidateList from './LessonCandidateList.svelte';
	import { LESSON_CANDIDATES } from '$lib/test-support/improvementFixtures';

	const { Story } = defineMeta({
		title: 'Improvement/LessonCandidateList',
		component: LessonCandidateList,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	const decided = new SvelteMap<number, 'approved' | 'rejected'>();

	async function onApprove(id: number): Promise<void> {
		await new Promise((resolve) => setTimeout(resolve, 10));
		decided.set(id, 'approved');
	}

	async function onReject(id: number): Promise<void> {
		await new Promise((resolve) => setTimeout(resolve, 10));
		decided.set(id, 'rejected');
	}
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByTestId('lesson-action-effect')).toHaveTextContent(
			/Approve sends a rule to later sessions of its project/
		);
		const groups = canvas.getAllByRole('heading', { level: 3 }).map((h) => h.textContent?.trim());
		await expect(groups).toEqual(['night-watch-notes', 'temet-proxy', 'No project']);
		await expect(canvas.getByText('2 conversations · 2 lessons')).toBeInTheDocument();
		const conversations = canvas
			.getAllByRole('heading', { level: 4 })
			.map((h) => h.textContent?.trim());
		await expect(conversations).toEqual([
			'Redesign the lesson review cards',
			'Review and refine system prompt with Mike MCP',
			'Fix the capture-DB retry loop',
			'Tidy the release notes'
		]);

		const cards = canvas.getAllByRole('article');
		await expect(cards).toHaveLength(4);
		await expect(
			within(cards[1]!).getByText('Leave the code better than you found it.')
		).toBeInTheDocument();
		await expect(canvas.getByText('· from a directive you approved')).toBeInTheDocument();

		await userEvent.click(within(cards[1]!).getByRole('button', { name: 'Approve' }));
		await waitFor(() => expect(within(cards[1]!).getByText('Approved')).toBeInTheDocument());
		await expect(canvas.getAllByRole('article')).toEqual(cards);
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<LessonCandidateList
				candidates={LESSON_CANDIDATES}
				error={null}
				{decided}
				detailHrefFor={(id) => `/sessions/improvement/lessons/${id}`}
				{onApprove}
				{onReject}
				onRetry={() => {}}
			/>
		</div>
	{/snippet}
</Story>
