<!--
Two lesson bands in one frame, as the Lessons panel stacks them: approving the first while it is
collapsed must not move the second; expanding the second shows the ask and the full correction,
and rejecting it with a reason leaves the band in place showing "Rejected".
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import LessonCandidateCard from './LessonCandidateCard.svelte';
	import type { CandidateState } from './LessonCandidateCard.svelte';
	import { LESSON_CANDIDATES } from '$lib/test-support/improvementFixtures';
	import type { RejectReason } from '$lib/api/improvement';

	const { Story } = defineMeta({
		title: 'Improvement/LessonCandidateCard',
		component: LessonCandidateCard,
		tags: ['autodocs']
	});

	const FIRST = LESSON_CANDIDATES[2]!;
	const SECOND = LESSON_CANDIDATES[1]!;
	const approveSpy = fn();
	const rejectSpy = fn();

	function pause(ms: number): Promise<void> {
		return new Promise((resolve) => setTimeout(resolve, ms));
	}
</script>

<script lang="ts">
	let states = $state<Record<number, CandidateState>>({});

	async function approve(id: number): Promise<void> {
		approveSpy(id);
		await pause(150);
		states[id] = 'approved';
	}

	async function reject(id: number, reason: RejectReason, note?: string): Promise<void> {
		rejectSpy(id, reason, note);
		await pause(20);
		states[id] = 'rejected';
	}
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const [firstCard, secondCard] = canvas.getAllByRole('article');
		const first = within(firstCard!);
		const second = within(secondCard!);

		await expect(first.getByText(FIRST.text)).toBeInTheDocument();
		await expect(first.getByText('Preference')).toBeInTheDocument();
		await expect(first.getByText('2 corrections')).toBeInTheDocument();
		await expect(second.getByText(/from your reply in/)).toBeInTheDocument();

		const topBefore = secondCard!.getBoundingClientRect().top;
		const approveBtn = first.getByRole('button', { name: 'Approve' });
		await userEvent.click(approveBtn);
		await expect(approveBtn).toHaveAttribute('aria-busy', 'true');
		await expect(approveBtn).toHaveTextContent('Approve');
		await expect(approveBtn).toBeDisabled();
		await waitFor(() => expect(first.getByText('Approved')).toBeInTheDocument());
		await expect(approveSpy).toHaveBeenCalledWith(FIRST.id);
		await expect(firstCard).toHaveAttribute('data-state', 'approved');
		await expect(secondCard!.getBoundingClientRect().top).toBe(topBefore);

		const toggle = second.getByRole('button', { name: SECOND.text });
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await expect(second.queryByText(/paste the verdict line/)).not.toBeInTheDocument();
		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');
		await expect(
			second.getByText(/Find out why, fix it, and verify the fix\./)
		).toBeInTheDocument();
		await expect(second.getByText(/paste the verdict line\.$/)).toBeInTheDocument();
		await expect(second.getByText(/2 earlier corrections back this rule/)).toBeInTheDocument();
		await expect(second.getByRole('link', { name: 'Open the full lesson' })).toHaveAttribute(
			'href',
			`/sessions/improvement/lessons/${SECOND.id}`
		);

		await userEvent.click(second.getByRole('button', { name: 'Reject' }));
		await expect(second.getByRole('group', { name: 'Why reject it?' })).toBeInTheDocument();
		await userEvent.click(second.getByRole('button', { name: 'Just wrong' }));
		await waitFor(() => expect(second.getByText('Rejected')).toBeInTheDocument());
		await expect(rejectSpy).toHaveBeenCalledWith(SECOND.id, 'wrong', undefined);
		await expect(canvas.getAllByRole('article')).toHaveLength(2);
		await expect(secondCard).toBeInTheDocument();
		await expect(second.queryByRole('button', { name: 'Reject' })).not.toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<section class="edge mech-group" aria-label="Lessons waiting on you">
				{#each [FIRST, SECOND] as candidate (candidate.id)}
					<LessonCandidateCard
						{candidate}
						detailHref={`/sessions/improvement/lessons/${candidate.id}`}
						decision={states[candidate.id] ?? 'waiting'}
						busy={false}
						onApprove={() => approve(candidate.id)}
						onReject={(reason, note) => reject(candidate.id, reason, note)}
					/>
				{/each}
			</section>
		</div>
	{/snippet}
</Story>
