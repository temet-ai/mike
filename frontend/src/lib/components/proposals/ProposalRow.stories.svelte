<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import ProposalRow from './ProposalRow.svelte';
	import {
		PROPOSAL_AGENT,
		PROPOSAL_EXISTING_RULE,
		PROPOSAL_NEW_RULE
	} from '$lib/test-support/proposalsFixtures';

	const onDecide = fn();

	const { Story } = defineMeta({
		title: 'Proposals/ProposalRow',
		component: ProposalRow,
		tags: ['autodocs']
	});
</script>

<Story
	name="Rule and agent proposals"
	play={async ({ canvasElement }) => {
		onDecide.mockClear();
		const canvas = within(canvasElement);
		const fresh = within(canvas.getByTestId('new-rule'));
		await expect(fresh.getByText('Waiting on you')).toBeInTheDocument();
		await expect(fresh.getByText('.claude/rules/dependency-pins.md')).toBeInTheDocument();
		await expect(fresh.getByText(/^Creates/)).toBeInTheDocument();
		await expect(fresh.getByText(/# Dependency pins/)).toBeInTheDocument();
		await expect(
			fresh.getByText('no, put the tag pin back, the release build cannot see your local path')
		).toBeInTheDocument();
		await expect(fresh.getByRole('button', { name: 'Undo decision' })).toBeDisabled();
		await userEvent.click(fresh.getByRole('button', { name: 'Accept' }));
		await expect(onDecide).toHaveBeenCalledWith('accept');

		const accepted = within(canvas.getByTestId('existing-rule'));
		await expect(accepted.getByText('.claude/rules/frontend-copy.md')).toBeInTheDocument();
		await expect(accepted.getByText(/^Adds to/)).toBeInTheDocument();
		await expect(accepted.getByRole('button', { name: 'Accept' })).toBeDisabled();
		await expect(
			accepted.getByText(/add the text above to \.claude\/rules\/frontend-copy\.md yourself/)
		).toBeInTheDocument();

		const agent = within(canvas.getByTestId('agent'));
		await expect(agent.getByText('.claude/agents/release-reviewer.md')).toBeInTheDocument();
		await expect(
			agent.getByText(/never changes an agent definition on its own/)
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="flex w-[52rem] flex-col gap-3 bg-[var(--surface-bg)] p-4">
			<div data-testid="new-rule"><ProposalRow proposal={PROPOSAL_NEW_RULE} {onDecide} /></div>
			<div data-testid="existing-rule">
				<ProposalRow proposal={PROPOSAL_EXISTING_RULE} {onDecide} />
			</div>
			<div data-testid="agent"><ProposalRow proposal={PROPOSAL_AGENT} {onDecide} /></div>
		</div>
	{/snippet}
</Story>
