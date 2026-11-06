<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import JudgeConfigurationEdit from './JudgeConfigurationEdit.svelte';
	import {
		BUNDLES,
		GATES,
		TURN_RUNS,
		WORDING_BANNED,
		textOutsideCode
	} from '$lib/test-support/judgesFixtures';

	const TURN_JUDGE_BUNDLES = BUNDLES.filter((b) => b.role === 'turn_judge');
	const onConfirm = fn(async () => {});

	const { Story } = defineMeta({
		title: 'Labelling/JudgeConfigurationEdit',
		component: JudgeConfigurationEdit,
		tags: ['autodocs'],
		args: {
			role: 'turn_judge',
			bundles: TURN_JUDGE_BUNDLES,
			liveId: 14,
			runs: TURN_RUNS,
			gates: GATES,
			open: true,
			initial: 19,
			onConfirm
		}
	});
</script>

<Story
	name="Default"
	play={async ({ args }) => {
		const body = within(document.body);

		await waitFor(() =>
			expect(
				body.getByRole('heading', { name: "Change the turn judge's configuration" })
			).toBeInTheDocument()
		);

		const option19 = document.querySelector(
			'input[type="radio"][value="19"]'
		) as HTMLInputElement;
		await expect(option19.checked).toBe(true);

		await expect(body.getByTestId('confirm-words').textContent).toContain(
			'Makes configuration 19 the one that labels new turns'
		);

		const barLines = [...document.querySelectorAll('.jce-option')].map((el) =>
			el.textContent?.includes('accuracy bar set for being put in use')
		);
		await expect(barLines.every(Boolean)).toBe(true);

		await userEvent.click(body.getByRole('button', { name: 'Use configuration 19' }));
		await expect(args.onConfirm).toHaveBeenCalledWith(19);

		await waitFor(() =>
			expect(body.getByTestId('rejudge-next').textContent).toContain('mike eval rejudge --execute')
		);

		const dialog = document.querySelector('[data-slot="dialog-content"]') as HTMLElement;
		await expect(textOutsideCode(dialog)).not.toMatch(WORDING_BANNED);
	}}
/>
