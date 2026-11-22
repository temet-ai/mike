<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import {
		CONFLICT_REASON,
		actionFixture,
		healthyProblems,
		problemFixture
	} from '../__fixtures__/wright';
	import WrightActionRow from './WrightActionRow.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightActionRow',
		component: WrightActionRow,
		tags: ['autodocs']
	});

	const onUndo = fn();
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(
			canvas.getByRole('button', { name: /^Undo: Tell undelivered senders, action 43/ })
		);
		await expect(onUndo).toHaveBeenCalledWith(43);
		await expect(canvas.getAllByRole('alert')[0]).toHaveTextContent(
			`Couldn't undo: ${CONFLICT_REASON}`
		);
		await expect(canvas.getByText('Undone')).toBeVisible();
		await expect(
			canvas.getByText('The problem passed its check again after this action.')
		).toBeVisible();
		await expect(canvas.getByText('Undo blocked')).toBeVisible();
	}}
>
	{#snippet template()}
		<ul
			class="edge"
			style="list-style: none; margin: 0; padding: 0; max-inline-size: 44rem; --edge-surface: var(--surface-card);"
		>
			<WrightActionRow
				action={actionFixture({
					id: 43,
					remedy: 'tell_undelivered_senders',
					state: 'applied',
					can_undo: true,
					verified_at: null
				})}
				problem={problemFixture()}
				{onUndo}
			/>
			<WrightActionRow
				action={actionFixture({
					id: 44,
					remedy: 'tell_undelivered_senders',
					state: 'applied',
					can_undo: true,
					verified_at: null
				})}
				undo={{ state: 'failed', reason: CONFLICT_REASON }}
				{onUndo}
			/>
			<WrightActionRow action={actionFixture({ id: 45, state: 'undone' })} {onUndo} />
			<WrightActionRow
				action={actionFixture({ id: 46, problem_id: 72 })}
				problem={healthyProblems()[0]}
				{onUndo}
			/>
			<WrightActionRow action={actionFixture({ id: 47, state: 'undo_conflict' })} {onUndo} />
		</ul>
	{/snippet}
</Story>
