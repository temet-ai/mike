<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, fn } from 'storybook/test';
	import DecisionActionCard from './DecisionActionCard.svelte';
	import type { ConsolidationAction } from '$lib/api/memoryDecisions';

	const { Story } = defineMeta({
		title: 'Memory/DecisionActionCard',
		component: DecisionActionCard,
		tags: ['autodocs']
	});

	const ACTION: ConsolidationAction = {
		id: 68,
		run_id: 12,
		run_kind: 'session',
		lesson_id: 68,
		lesson_uuid: '018f7b2a-0000-7000-8000-000000000068',
		project_id: 3,
		project_path: '/Users/dev/temet-proxy',
		session_id: 501,
		session_uuid: '01j9x4qf7h8k2n3p5r6t7v8w9a',
		source_turn_id: 812,
		files_touched: ['crates/mike/src/store/persistence/lessons.rs'],
		agent_id: 4,
		lesson_text:
			'FIXED 2026-07-19: the seeded DB credential was postgres:postgres, not the placeholder in .env.example — corrected to postgres:postgres and confirmed the migration runner connects.',
		action_kind: 'archived_duplicate',
		outcome: 'applied',
		error: null,
		before_stage: 'daily',
		after_stage: 'archived',
		decided_by: 'embedding_distance',
		distance: 0.0421,
		threshold: 0.85,
		embedding_model: 'text-embedding-3-small',
		counterpart_lesson_id: 66,
		counterpart_uuid: '018f7b2a-0000-7000-8000-000000000066',
		counterpart_text:
			'DB credential appears broken in local dev — postgres auth rejects the .env.example placeholder. Needs investigation.',
		runners_up: [{ lesson_id: 61, distance: 0.79 }],
		rank_detail: { consolidation_count: 3, excess: 1 },
		injection_count: 4,
		created_at: '2026-07-19T09:13:00Z'
	};
</script>

<Story
	name="Default"
	args={{ action: ACTION, onRestore: fn() }}
	play={async ({ args, canvasElement }) => {
		const canvas = within(canvasElement);
		const restoreButton = canvas.getByRole('button', { name: /restore this lesson/i });
		await userEvent.click(restoreButton);
		await expect(args.onRestore).toHaveBeenCalledWith(ACTION);
	}}
>
	{#snippet template(args)}
		<div style="max-width: 720px; padding: 12px;">
			<DecisionActionCard {...args} />
		</div>
	{/snippet}
</Story>
