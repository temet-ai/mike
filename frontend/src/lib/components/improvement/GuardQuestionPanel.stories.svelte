<!--
The rail card for one turn-guard question: mode, holdback readouts and progress, and the honest
holdback-vs-treated effect verdict. Its nudges are listed beside it by the page, never inside it.
See GuardQuestionPanel.svelte's own doc comment.
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import GuardQuestionPanel from './GuardQuestionPanel.svelte';
	import type { GuardEffectEstimate } from '$lib/api/guardEffect';

	const EFFECT: GuardEffectEstimate = {
		questionKey: 'relapse',
		asOfSecs: 1790600000,
		primary: {
			treated: { n: 120, bad: 17, pBad: 0.14 },
			holdback: { n: 40, bad: 11, pBad: 0.27 },
			rd: 0.13,
			ci: [0.03, 0.19]
		},
		secondary: {
			treated: { n: 120, bad: 17, pBad: 0.14 },
			holdback: { n: 40, bad: 11, pBad: 0.27 },
			rd: 0.13,
			ci: [0.02, 0.2]
		},
		perProtocolNotRandomised: {
			treated: { n: 118, bad: 16, pBad: 0.136 },
			holdback: { n: 40, bad: 11, pBad: 0.27 },
			rd: 0.134,
			ci: [0.03, 0.2]
		},
		missing: {},
		events: 160,
		status: 'readable'
	};

	const { Story } = defineMeta({
		title: 'Improvement/GuardQuestionPanel',
		component: GuardQuestionPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			canvas.getByRole('heading', { name: 'Live — starts a new turn' })
		).toBeInTheDocument();
		await expect(
			canvas.getByText('This question is live — it can start a new turn for the model.')
		).toBeInTheDocument();
		await expect(canvas.getByText('Fired, last 24 hours')).toBeInTheDocument();
		await expect(canvas.getByText('Held back to compare')).toBeInTheDocument();
		await expect(canvas.getByText('Delivered as usual')).toBeInTheDocument();
		await expect(canvas.getByText('9')).toBeInTheDocument();
		await expect(canvas.getByText('40 / 34')).toBeInTheDocument();
		await expect(canvas.getByText('120')).toBeInTheDocument();
		await expect(
			canvas.getByText(/Fewer corrections in the treated arm: 14% of 120 fires delivered as usual/)
		).toBeInTheDocument();
		await expect(canvas.getByTestId('guard-marks-effect')).toHaveTextContent(
			/If too many are wrong, the question goes back to being tried quietly/
		);
		// The nudges are listed beside this card by the page; none render inside it.
		await expect(canvas.queryByTestId('guard-wake-list')).not.toBeInTheDocument();
		await expect(canvas.queryByRole('button', { name: 'Right' })).not.toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="max-w-[24rem] bg-[var(--surface-bg)] p-4">
			<GuardQuestionPanel
				questionKey="relapse"
				mode="wake"
				counts24h={{ rows: 40, wouldFire: 9 }}
				effect={EFFECT}
				readPointHoldback={34}
			/>
		</div>
	{/snippet}
</Story>
