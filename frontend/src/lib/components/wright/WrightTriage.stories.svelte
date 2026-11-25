<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, screen, userEvent, waitFor, within } from 'storybook/test';
	import type { WrightProblem } from '$lib/api/wright';
	import { byPriorityDesc } from '$lib/wright/causes';
	import { emptyFeed } from '$lib/wright/feed';
	import {
		GROUPS,
		MAX_ATTEMPTS,
		WAITED_HOURS,
		ago,
		causeFixture,
		openProblems
	} from '../__fixtures__/wright';
	import { openDialog } from '../__fixtures__/dialogs';
	import WrightTriage from './WrightTriage.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightTriage',
		component: WrightTriage,
		tags: ['autodocs'],
		parameters: { layout: 'padded' }
	});

	/** In the engineer's order: the pinned cause first, then by rank. */
	const CAUSES = [
		causeFixture({
			cause_id: 1,
			check_name: 'decisions_health',
			resource: 'blocked_dialog',
			problems: 16,
			opened_at: ago(60 * 30),
			priority: 90
		}),
		causeFixture({
			cause_id: 2,
			check_name: 'slow_queries',
			resource: 'query 1: SELECT * FROM wright.problems WHERE state = $1',
			problems: 3,
			opened_at: ago(60 * 3),
			priority: 50,
			pinned: true,
			pinned_at: ago(10)
		}),
		causeFixture({
			cause_id: 3,
			check_name: 'idle_polls',
			resource: 'query 2: UPDATE wright.causes SET notified_at = now()',
			problems: 1,
			opened_at: ago(60 * 3),
			priority: 70
		}),
		causeFixture({
			cause_id: 4,
			check_name: 'log_errors',
			resource: 'mike_features::proxy::responses_capture::record: frame too large',
			problems: 2,
			opened_at: ago(60 * 3),
			priority: 60
		}),
		causeFixture({
			cause_id: 5,
			check_name: 'scheduled_job_failures',
			resource: 'job:decisions_loop@3dcd7aa2-3126-4122-84e0-051a00751c02',
			problems: 1,
			opened_at: ago(60 * 3),
			priority: 50
		}),
		causeFixture({
			cause_id: 6,
			check_name: 'slow_queries',
			resource: 'query 3: SELECT * FROM memory.operator_turns',
			problems: 4,
			opened_at: ago(60 * 3),
			priority: 40,
			group_id: 3,
			group_name: 'Mail delivery'
		}),
		causeFixture({
			cause_id: 7,
			check_name: 'undelivered_mail',
			resource: '3dcd7aa2-3126-4122-84e0-051a00751c02',
			problems: 1,
			opened_at: ago(60 * 3),
			priority: 10
		})
	].sort(byPriorityDesc);

	const problems: Record<number, ReturnType<typeof emptyFeed<WrightProblem>>> = {};
	problems[1] = emptyFeed();
	problems[1].status = 'ready';
	problems[1].rows = openProblems();

	const ok = async () => true;
	const onPin = fn(async (_id: number, _pinned: boolean) => true);
	const onMove = fn(async (_id: number, _priority: number | null) => true);
	const onMerge = fn(async (_root: number, _ids: number[]) => true);
	const onSeparate = fn(ok);
	const onGroup = fn(async (_ids: number[], _target: unknown) => true);
	const onOpenCause = fn();
	const onView = fn();
	const names = (root: HTMLElement): string[] =>
		[...root.querySelectorAll('[data-slot="wright-cause-row"]')].map((li) => li.id);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		// Ranked: the engineer's order, the pinned cause first.
		const ranked = canvas.getByRole('list', {
			name: 'Causes in the order the engineer takes them'
		});
		await expect(names(ranked)).toEqual([
			'cause-2',
			'cause-1',
			'cause-3',
			'cause-4',
			'cause-5',
			'cause-6',
			'cause-7'
		]);

		// The selection bar is in the layout from the start: selecting rows moves nothing.
		const bar = canvasElement.querySelector('.tri__bar') as HTMLElement;
		const before = bar.getBoundingClientRect().height;
		const listTop = ranked.getBoundingClientRect().top;
		await expect(canvas.getByText('0 selected')).not.toBeVisible();
		await userEvent.click(within(canvasElement.querySelector('#cause-3')!).getByRole('checkbox'));
		await userEvent.click(within(canvasElement.querySelector('#cause-4')!).getByRole('checkbox'));
		await expect(canvas.getByText('2 selected')).toBeVisible();
		await expect(bar.getBoundingClientRect().height).toBe(before);
		await expect(ranked.getBoundingClientRect().top).toBe(listTop);

		// Merge into: the dialog says which cause keeps the others and what Separate can undo.
		await userEvent.click(canvas.getByRole('button', { name: 'Merge into' }));
		const merge = within(await openDialog('Merge 2 causes'));
		await expect(merge.getByRole('radio', { name: /idle_polls/ })).toBeChecked();
		await userEvent.click(merge.getByRole('button', { name: 'Merge' }));
		await expect(onMerge).toHaveBeenCalledWith(3, [4]);
		await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
		await expect(canvas.getByText('0 selected')).not.toBeVisible();

		// Group on one row opens the group dialog for that cause.
		await userEvent.click(canvas.getByRole('button', { name: 'Add log_errors to a group' }));
		await openDialog('Group this cause');
		await userEvent.click(
			within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' })
		);
		await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

		// Move up and down work from the keyboard, inside the pin state of the row.
		const four = within(canvasElement.querySelector('#cause-4') as HTMLElement);
		await expect(
			within(canvasElement.querySelector('#cause-2') as HTMLElement).getByRole('button', {
				name: 'Move slow_queries up'
			})
		).toBeDisabled();
		await expect(
			within(canvasElement.querySelector('#cause-1') as HTMLElement).getByRole('button', {
				name: 'Move decisions_health up'
			})
		).toBeDisabled();
		const down = four.getByRole('button', { name: 'Move log_errors down' });
		down.focus();
		await userEvent.keyboard('{Enter}');
		await expect(onMove).toHaveBeenLastCalledWith(4, 45);
		await userEvent.click(canvas.getByRole('button', { name: 'Pin undelivered_mail' }));
		await expect(onPin).toHaveBeenCalledWith(7, true);

		// Dragging a row onto the top half of another one is the same move.
		const grip = canvasElement.querySelector('#cause-7 .cause__grip') as HTMLElement;
		const target = canvasElement.querySelector('#cause-3') as HTMLElement;
		const transfer = new DataTransfer();
		grip.dispatchEvent(new DragEvent('dragstart', { bubbles: true, dataTransfer: transfer }));
		const box = target.getBoundingClientRect();
		const at = { bubbles: true, cancelable: true, dataTransfer: transfer, clientY: box.top + 2 };
		target.dispatchEvent(new DragEvent('dragover', at));
		await waitFor(() => expect(target).toHaveAttribute('data-drop', 'before'));
		target.dispatchEvent(new DragEvent('drop', at));
		await expect(onMove).toHaveBeenLastCalledWith(7, 80);
		await waitFor(() => expect(target).not.toHaveAttribute('data-drop'));

		// By area: a named group first, then the derived areas, each ordered by its best cause.
		await userEvent.click(canvas.getByRole('tab', { name: 'By area' }));
		await expect(onView).toHaveBeenLastCalledWith('area');
		const headings = canvas.getAllByRole('heading', { level: 3 });
		await expect(headings.map((h) => h.querySelector('.tri__section-name')?.textContent)).toEqual([
			'Mail delivery',
			'wright tables',
			'decisions',
			'mike_features::proxy::responses_capture',
			'decisions_loop',
			'undelivered_mail'
		]);
		// The two checks on wright.* tables show together, the pinned one first.
		const tables = canvas.getByRole('list', { name: 'Causes in wright tables' });
		await expect(names(tables)).toEqual(['cause-2', 'cause-3']);
		// Moving inside a section passes only its own neighbours, and never a pinned cause.
		await expect(
			within(tables.querySelector('#cause-3') as HTMLElement).getByRole('button', {
				name: 'Move idle_polls up'
			})
		).toBeDisabled();
		// Rows are not draggable here: the section is not the whole queue.
		await expect(canvasElement.querySelector('.cause__grip--on')).toBeNull();
	}}
>
	{#snippet template()}
		<div
			class="edge"
			style="--edge-line: var(--border-subtle); --edge-surface: var(--surface-card); padding: var(--space-3); max-inline-size: 64rem;"
		>
			<WrightTriage
				causes={CAUSES}
				status="ready"
				waitedHours={WAITED_HOURS}
				groups={GROUPS}
				maxAttempts={MAX_ATTEMPTS}
				causeProblems={problems}
				paired={true}
				{onView}
				{onPin}
				{onMove}
				{onMerge}
				{onSeparate}
				{onGroup}
				{onOpenCause}
				onRetryProblems={() => {}}
				onShowHeldProblems={() => {}}
				onOpenProblem={() => {}}
			/>
		</div>
	{/snippet}
</Story>
