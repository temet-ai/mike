<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, fn } from 'storybook/test';
	import ArchivedLessonCard from './ArchivedLessonCard.svelte';
	import type { ArchivedLesson } from '$lib/api/memoryDecisions';

	const { Story } = defineMeta({
		title: 'Memory/ArchivedLessonCard',
		component: ArchivedLessonCard,
		tags: ['autodocs']
	});

	const LESSON: ArchivedLesson = {
		id: 55,
		uuid: '018f7b2a-0000-7000-8000-000000000055',
		project_id: 3,
		project_path: '/Users/dev/temet-proxy',
		kind: 'correction',
		text: 'A recurrence on a different worker with inverted direction, explicitly recording that the earlier guards were necessary but incomplete — the evidence the earlier fix failed.',
		topic: 'db-safety',
		session_id: 501,
		session_uuid: '01j9x4qf7h8k2n3p5r6t7v8w9a',
		source_turn_id: 812,
		agent_id: 4,
		created_at: '2026-06-16T09:00:00Z',
		archived_at: '2026-06-19T03:00:00Z',
		archived_reason: 'duplicate',
		duplicate_of_id: 53
	};
</script>

<Story
	name="Default"
	args={{ lesson: LESSON, onRestore: fn() }}
	play={async ({ args, canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText(/merged as a duplicate/i)).toBeInTheDocument();
		await expect(canvas.getByText('duplicate of lesson #53')).toBeInTheDocument();
		await expect(canvas.getByText('db-safety')).toBeInTheDocument();
		await expect(canvas.getByText(LESSON.text)).toBeInTheDocument();

		const restoreButton = canvas.getByRole('button', { name: /restore this lesson/i });
		await userEvent.click(restoreButton);
		await expect(args.onRestore).toHaveBeenCalledWith(LESSON);
	}}
>
	{#snippet template(args)}
		<div style="max-width: 720px; padding: 12px;">
			<ArchivedLessonCard {...args} />
		</div>
	{/snippet}
</Story>
