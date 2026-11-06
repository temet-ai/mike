<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import JudgeSettings from './JudgeSettings.svelte';
	import {
		JUDGE_SETTINGS,
		WORDING_BANNED,
		textOutsideCode
	} from '$lib/test-support/judgesFixtures';
	import { TURN_JUDGE_SETTING_KEYS } from './judges';

	const onSave = fn();

	const { Story } = defineMeta({
		title: 'Labelling/JudgeSettings',
		component: JudgeSettings,
		tags: ['autodocs'],
		args: {
			keys: TURN_JUDGE_SETTING_KEYS,
			modelKey: 'judge_worker.model',
			promptKey: 'judge_worker.system_prompt',
			settings: JUDGE_SETTINGS,
			liveId: 14,
			loading: {},
			errors: {},
			inheritLabel: "the steerer's model",
			onSave
		}
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement, args }) => {
		const c = within(canvasElement);

		await expect(canvasElement.textContent).toContain('Wait after a turn closes (ms)');
		await expect(canvasElement.textContent).toContain('Days of turns the sweep covers');
		await expect(canvasElement.textContent).toContain('Re-judge calls per day');
		await expect(canvasElement.textContent).not.toContain('Live turn-judge bundle');
		await expect(canvasElement.textContent).not.toContain('Configuration in use');

		await expect(canvasElement.textContent).not.toContain('Instructions');
		await expect(c.queryByText('google/gemini-3.8-flash')).toBeNull();

		await userEvent.click(
			c.getByRole('button', {
				name: 'Not used while configuration 14 is in use: model, instructions'
			})
		);
		await expect(canvasElement.textContent).toContain('Instructions');
		await expect(canvasElement.textContent).toContain('google/gemini-3.8-flash');

		const toggle = c.getByRole('switch', { name: /^Judge a turn as soon as it is answered/ });
		await userEvent.click(toggle);
		await expect(args.onSave).toHaveBeenCalledWith('turn_judge.immediate', false);

		await expect(textOutsideCode(canvasElement)).not.toMatch(WORDING_BANNED);
	}}
/>
