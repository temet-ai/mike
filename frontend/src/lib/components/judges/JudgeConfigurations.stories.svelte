<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import JudgeConfigurations from './JudgeConfigurations.svelte';
	import {
		BUNDLES,
		TURN_RUNS,
		WORDING_BANNED,
		textOutsideCode
	} from '$lib/test-support/judgesFixtures';

	const TURN_JUDGE_BUNDLES = BUNDLES.filter((b) => b.role === 'turn_judge');

	const { Story } = defineMeta({
		title: 'Labelling/JudgeConfigurations',
		component: JudgeConfigurations,
		tags: ['autodocs'],
		args: {
			bundles: TURN_JUDGE_BUNDLES,
			liveId: 14,
			runs: TURN_RUNS
		}
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);

		await expect(c.queryByRole('table')).toBeNull();

		await userEvent.click(c.getByRole('button', { name: /Configurations · 4/ }));
		await expect(c.getByRole('table')).toBeInTheDocument();

		const row14 = canvasElement.querySelector('[data-config="14"]') as HTMLElement;
		await expect(row14.textContent).toContain('in use');

		const row15 = canvasElement.querySelector('[data-config="15"]') as HTMLElement;
		await expect(row15.textContent).toContain(
			'same model and instructions as 14; its score differs because it was measured on different turns'
		);

		const row1 = canvasElement.querySelector('[data-config="1"]') as HTMLElement;
		await expect(row1.textContent).toContain('not measured');

		const row19 = canvasElement.querySelector('[data-config="19"]') as HTMLElement;
		await expect(
			within(row19).getByRole('link', { name: /^28 Sep( 2026)? \d\d:\d\d$/ })
		).toHaveAttribute('href', expect.stringMatching(/\/sessions\/labelling\/accuracy\/5$/));

		await expect(textOutsideCode(canvasElement)).not.toMatch(WORDING_BANNED);
	}}
/>
