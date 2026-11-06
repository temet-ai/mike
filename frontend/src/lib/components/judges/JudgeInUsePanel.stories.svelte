<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import JudgeInUsePanel from './JudgeInUsePanel.svelte';
	import {
		BUNDLES,
		GATES,
		TURN_RUNS,
		WORDING_BANNED,
		textOutsideCode
	} from '$lib/test-support/judgesFixtures';

	const BUNDLE_14 = BUNDLES.find((b) => b.id === 14) ?? null;
	const onEdit = fn();

	const { Story } = defineMeta({
		title: 'Labelling/JudgeInUsePanel',
		component: JudgeInUsePanel,
		tags: ['autodocs'],
		args: {
			role: 'turn_judge',
			bundle: BUNDLE_14,
			liveId: 14,
			runs: TURN_RUNS,
			gates: GATES,
			when: '1.5 s after you reply',
			keyConfigured: true,
			keyError: null,
			canEdit: true,
			onEdit
		}
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement, args }) => {
		await expect(canvasElement.textContent).not.toContain('in use since');
		await expect(canvasElement.textContent).toContain('google/gemini-3.8-flash, thinking: low');
		await expect(canvasElement.textContent).toContain('Dated 23 Sep 2026');
		await expect(canvasElement.textContent).toContain('1.5 s after you reply');
		await expect(canvasElement.textContent).toContain(
			'Put in use before its accuracy was measured.'
		);
		await expect(canvasElement.textContent).toContain(
			'it passed the accuracy bar set for being put in use. When it says Corrected, the label agrees 86 times in 100 (needs 85).'
		);
		await expect(canvasElement.textContent).toMatch(/set\s+up/);

		const c = within(canvasElement);
		await userEvent.click(c.getByRole('button', { name: 'Edit' }));
		await expect(args.onEdit).toHaveBeenCalled();

		await expect(textOutsideCode(canvasElement)).not.toMatch(WORDING_BANNED);
	}}
/>
