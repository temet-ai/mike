<!--
One delivered guard nudge as a compact band: the question, "Was this right?" beside it, and a
source line with the outcome tag. Expanding shows the note the guard added in full. Answering
keeps the band where it is. The not-judged and no-recorded-note states are covered by the
GuardWakeList story.
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, expect, waitFor, within } from 'storybook/test';
	import GuardWakeCard from './GuardWakeCard.svelte';
	import { GUARD_WAKES } from '$lib/test-support/improvementFixtures';

	const { Story } = defineMeta({
		title: 'Improvement/GuardWakeCard',
		component: GuardWakeCard,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	const NOTE =
		'You told the operator the migration shipped, but nothing in this turn ran it.\n\nRun it and show the output before you say it is done.';
	let wake = $state({ ...GUARD_WAKES[0]!, noteText: NOTE });
	let busy = $state(false);

	async function onLabel(label: 'right' | 'wrong'): Promise<void> {
		busy = true;
		await new Promise((resolve) => setTimeout(resolve, 150));
		wake = { ...wake, operatorLabel: label };
		busy = false;
	}
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = canvas.getByRole('button', { name: "Did the last turn's work actually ship?" });
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await expect(canvas.getByText('Woke the model')).toBeInTheDocument();
		await expect(
			canvas.getByRole('link', { name: 'Investigate slow 40-minute Flux job' })
		).toHaveAttribute('href', '/sessions/hash-99aa11');
		await expect(canvas.getByText('Next turn changed plan')).toBeInTheDocument();
		await expect(canvas.getByText('Was this right?')).toBeInTheDocument();
		await expect(canvas.queryByText(/Run it and show the output/)).not.toBeInTheDocument();

		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');
		await expect(canvas.getByText('What it told the model')).toBeInTheDocument();
		await expect(
			canvas.getByText(/Run it and show the output before you say it is done\./)
		).toBeInTheDocument();
		await expect(
			canvas.getByText(
				'The next turn changed direction — not because anything was wrong, just a change of plan.'
			)
		).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: 'Open the turn' })).toHaveAttribute(
			'href',
			'/sessions/hash-99aa11/requests/88'
		);

		const right = canvas.getByRole('button', { name: 'Right' });
		await userEvent.click(right);
		await expect(right).toHaveAttribute('aria-busy', 'true');
		await expect(right).toHaveTextContent('Right');
		await waitFor(() => expect(canvas.getByText('You said right')).toBeInTheDocument());
		await expect(canvas.queryByRole('button', { name: 'Right' })).not.toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<div class="edge mech-group">
				<GuardWakeCard {wake} {busy} {onLabel} />
			</div>
		</div>
	{/snippet}
</Story>
