<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import InjectedSteerCallout from './InjectedSteerCallout.svelte';

	const { Story } = defineMeta({
		title: 'Shell/InjectedSteerCallout',
		component: InjectedSteerCallout,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{
		payloadText:
			'You claimed the fix is done, but no test output was shown - run the tests and show the output.'
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText('Authored nudge')).toBeInTheDocument();
		const nudgeText = canvasElement.querySelector('.isc-steer__nudge-text');
		await expect(nudgeText?.textContent).toBe(
			'You claimed the fix is done, but no test output was shown - run the tests and show the output.'
		);

		const pre = canvasElement.querySelector('pre.isc-steer__pre');
		await expect(pre).toBeTruthy();
		await expect(pre?.textContent).toContain(
			"<system-reminder>Steering note from the operator's proxy:"
		);
		await expect(pre?.textContent).toContain(
			'You claimed the fix is done, but no test output was shown - run the tests and show the output.'
		);
		await expect(pre?.textContent).toContain(
			'If it is already done or does not apply, say so in a line.</system-reminder>'
		);
	}}
/>
