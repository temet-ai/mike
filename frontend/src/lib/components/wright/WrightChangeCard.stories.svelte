<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import WrightChangeCard from './WrightChangeCard.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightChangeCard',
		component: WrightChangeCard,
		tags: ['autodocs']
	});

	const onUndo = fn();
</script>

<Story
	name="Default"
	play={async ({ canvas }) => {
		await expect(canvas.getByText('Paused: Tell undelivered senders')).toBeVisible();
		await expect(canvas.getByText('Until the fleet closes')).toBeVisible();
		await userEvent.click(
			canvas.getByRole('button', { name: 'Undo: Paused: Tell undelivered senders' })
		);
		await expect(onUndo).toHaveBeenCalledOnce();
		await expect(canvas.getByText('Limit changed: Daily starts')).toBeVisible();
		await expect(canvas.getByRole('alert')).toHaveTextContent("Couldn't undo: Changed since.");
		await expect(canvas.getByText('Undone')).toBeVisible();

		// The queue changes name the causes and link to their rows.
		await expect(canvas.getByText('Pin cause 19')).toBeVisible();
		await expect(canvas.getByText('Merge causes 298 and 307 into 19')).toBeVisible();
		await expect(
			canvas.getByText('Put causes 19 and 318 in the group slow memory queries')
		).toBeVisible();
		await expect(canvas.getByText('Reset the order of cause 19')).toBeVisible();
		await expect(canvas.getByText('Separate merge 5')).toBeVisible();
		await expect(canvas.getByText('Take cause 7 out of its group')).toBeVisible();
		await expect(canvas.getByRole('link', { name: /Cause 298/ })).toHaveAttribute(
			'href',
			'#cause-298'
		);
	}}
>
	{#snippet template()}
		<div
			style="display: flex; flex-direction: column; gap: 8px; padding: 16px; max-inline-size: 40rem; background: var(--surface-card);"
		>
			<WrightChangeCard
				change={{ type: 'pause_remedy', name: 'tell_undelivered_senders', until: 'the fleet closes' }}
				undo="ready"
				{onUndo}
			/>
			<WrightChangeCard
				change={{ type: 'set_engineer_limit', field: 'daily_starts', value: 2 }}
				undo="failed"
				undoReason="Changed since."
				{onUndo}
			/>
			<WrightChangeCard
				change={{ type: 'open_problem', check: 'log_errors', resource: 'mike log' }}
				undo="undone"
				{onUndo}
			/>
			<WrightChangeCard change={{ kind: 'pin_cause', cause_id: 19 }} undo="ready" {onUndo} />
			<WrightChangeCard
				change={{ kind: 'merge_causes', into: 19, cause_ids: [298, 307] }}
				undo="ready"
				{onUndo}
			/>
			<WrightChangeCard
				change={{ kind: 'group_causes', name: 'slow memory queries', cause_ids: [19, 318] }}
				undo="ready"
				{onUndo}
			/>
			<WrightChangeCard
				change={{ kind: 'set_cause_priority', cause_id: 19, priority: null }}
				undo="ready"
				{onUndo}
			/>
			<WrightChangeCard change={{ kind: 'separate_merge', merge_id: 5 }} undo="ready" {onUndo} />
			<WrightChangeCard
				change={{ kind: 'ungroup_causes', cause_ids: [7] }}
				undo="ready"
				{onUndo}
			/>
		</div>
	{/snippet}
</Story>
