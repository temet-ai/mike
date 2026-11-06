<!--
Delivered guard nudges in one frame: the lead line, the nudges you can answer (newest first,
grouped by project and conversation), then those whose next turn has not been judged under "Not ready to answer yet". Answering one
keeps it in place.
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, expect, waitFor, within } from 'storybook/test';
	import GuardWakeList from './GuardWakeList.svelte';
	import { GUARD_WAKES } from '$lib/test-support/improvementFixtures';
	import type { GuardWake } from '$lib/api/guardEffect';

	const { Story } = defineMeta({
		title: 'Improvement/GuardWakeList',
		component: GuardWakeList,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let wakes = $state<GuardWake[]>(GUARD_WAKES.map((w) => ({ ...w })));

	async function onLabel(evaluationUuid: string, label: 'right' | 'wrong'): Promise<void> {
		await new Promise((resolve) => setTimeout(resolve, 10));
		wakes = wakes.map((w) =>
			w.evaluationUuid === evaluationUuid ? { ...w, operatorLabel: label } : w
		);
	}

	function onRetry(): void {}
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByTestId('guard-marks-effect')).toHaveTextContent(
			/goes back to being tried quietly/
		);
		const cards = canvas.getAllByRole('article');
		await expect(cards).toHaveLength(3);
		const projects = canvas.getAllByRole('heading', { level: 3 }).map((h) => h.textContent?.trim());
		await expect(projects).toEqual([
			'night-watch',
			'temet-proxy',
			'Not ready to answer yet',
			'temet-proxy'
		]);
		const conversations = canvas
			.getAllByRole('heading', { level: 4 })
			.map((h) => h.textContent?.trim());
		await expect(conversations).toEqual([
			'Investigate slow 40-minute Flux job',
			'Fix the capture-DB retry loop',
			'Redesign the lesson review cards'
		]);
		await expect(
			within(cards[0]!).queryByText('Investigate slow 40-minute Flux job')
		).not.toBeInTheDocument();
		await expect(within(cards[0]!).getByText('Next turn changed plan')).toBeInTheDocument();
		await expect(within(cards[1]!).getByText('Next turn corrected it')).toBeInTheDocument();
		await expect(
			within(cards[1]!).getByText('· 2 times in one turn, one answer covers all')
		).toBeInTheDocument();
		await expect(canvas.getByText('Not ready to answer yet')).toBeInTheDocument();
		await expect(canvas.getByText('1 — the next turn has not been judged')).toBeInTheDocument();
		await expect(within(cards[2]!).getByText('Ask once judged')).toBeInTheDocument();
		await expect(
			within(cards[2]!).queryByRole('button', { name: 'Right' })
		).not.toBeInTheDocument();

		const cardTop = cards[1]!.getBoundingClientRect().top;
		await userEvent.click(within(cards[1]!).getByRole('button', { name: 'Wrong' }));
		await waitFor(() => expect(within(cards[1]!).getByText('You said wrong')).toBeInTheDocument());
		await expect(cards[1]!.getBoundingClientRect().top).toBe(cardTop);
		await expect(canvas.getAllByRole('button', { name: 'Wrong' })).toHaveLength(1);
		await expect(canvas.getAllByRole('article')).toHaveLength(3);
		const toggle = within(cards[0]!).getByRole('button', {
			name: "Did the last turn's work actually ship?"
		});
		await userEvent.click(toggle);
		await expect(within(cards[0]!).getByText('What it told the model')).toBeInTheDocument();
		await expect(
			within(cards[0]!).getByText('The text of this note was not recorded.')
		).toBeInTheDocument();
		await expect(
			within(cards[0]!).getByText(
				'The next turn changed direction — not because anything was wrong, just a change of plan.'
			)
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<GuardWakeList
				{wakes}
				error={null}
				emptyText="No nudges are waiting for your answer."
				{onLabel}
				{onRetry}
			/>
		</div>
	{/snippet}
</Story>
