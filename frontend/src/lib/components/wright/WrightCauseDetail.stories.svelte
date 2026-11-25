<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, screen, userEvent, waitFor, within } from 'storybook/test';
	import type { WrightProblem } from '$lib/api/wright';
	import { emptyFeed } from '$lib/wright/feed';
	import {
		MAX_ATTEMPTS,
		absorbedFixture,
		ago,
		causeFixture,
		openProblems,
		problemFixture
	} from '../__fixtures__/wright';
	import { openDialog } from '../__fixtures__/dialogs';
	import WrightCauseDetail from './WrightCauseDetail.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightCauseDetail',
		component: WrightCauseDetail,
		tags: ['autodocs']
	});

	const CAUSE = causeFixture({
		cause_id: 50,
		check_name: 'slow_queries',
		resource:
			'query 1: SELECT * FROM memory.operator_turns t JOIN wright.problems p ON p.id = t.id',
		problems: 7,
		observations: 1733,
		opened_at: ago(60 * 52),
		priority: 14.2,
		pinned: true,
		pinned_at: ago(30),
		operator_priority: 30,
		investigable: false,
		not_investigated_because: 'finding',
		finding_id: 5,
		absorbed: [
			absorbedFixture(),
			absorbedFixture({
				merge_id: 6,
				cause_id: 32,
				check_name: 'idle_polls',
				key: 'query 9: SELECT 1 FROM wright.causes',
				problems: 2,
				by: 'engineer',
				can_separate: false
			})
		]
	});

	const feed = emptyFeed<WrightProblem>();
	feed.status = 'ready';
	feed.rows = openProblems();
	feed.held = [problemFixture({ id: 710, check_name: 'mcp_tool_errors' })];

	const onSeparate = fn(async (_id: number) => true);
	const onShowHeld = fn();
	const onOpenProblem = fn();
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		// Where it belongs and how it is ranked.
		await expect(canvas.getByText('memory tables')).toBeVisible();
		await expect(canvas.getByText('memory.operator_turns, wright.problems')).toBeVisible();
		await expect(canvas.getByText('Formula score')).toBeVisible();
		await expect(canvas.getByText('14.2')).toBeVisible();
		await expect(canvas.getByText('30, used instead')).toBeVisible();
		await expect(canvas.getByText('1733 (adds 7.5)')).toBeVisible();
		await expect(
			canvas.getByText('Pinned. The engineer will not take it: it already has a finding.')
		).toBeVisible();

		// Every open problem, and the ones that arrived while it was open wait to be asked for.
		const list = canvas.getByRole('list', { name: 'Open problems of cause 50' });
		await expect(list.querySelectorAll(':scope > [data-slot="wright-problem-row"]')).toHaveLength(
			2
		);
		await userEvent.click(canvas.getByRole('button', { name: '1 new' }));
		await expect(onShowHeld).toHaveBeenCalled();
		await userEvent.click(within(list).getAllByRole('button')[0]!);
		await expect(onOpenProblem).toHaveBeenCalledWith(707);
		await expect(canvas.getByText('Detail of problem 707')).toBeVisible();

		// Separate only where the operator made the merge.
		await expect(canvas.getByText(/merged by Wright's engineer/)).toBeVisible();
		await expect(canvas.getAllByRole('button', { name: /^Separate / })).toHaveLength(1);
		await userEvent.click(canvas.getByRole('button', { name: 'Separate slow_queries' }));
		const dialog = within(await openDialog('Separate this merge'));
		await expect(
			dialog.getByText(/Problems opened after the merge stay with slow_queries/)
		).toBeVisible();
		await userEvent.click(dialog.getByRole('button', { name: 'Separate' }));
		await expect(onSeparate).toHaveBeenCalledWith(5);
		await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
	}}
>
	{#snippet template()}
		<div style="max-inline-size: 56rem; padding: var(--space-3);">
			<WrightCauseDetail
				cause={CAUSE}
				maxAttempts={MAX_ATTEMPTS}
				problems={feed}
				{onSeparate}
				{onShowHeld}
				{onOpenProblem}
			>
				{#snippet problem(p)}
					<p>Detail of problem {p.id}</p>
				{/snippet}
			</WrightCauseDetail>
		</div>
	{/snippet}
</Story>
