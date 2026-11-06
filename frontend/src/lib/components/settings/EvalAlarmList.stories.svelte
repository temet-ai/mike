<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import EvalAlarmList from './EvalAlarmList.svelte';
	import { ALARMS, BUNDLES } from '$lib/test-support/judgesFixtures';
	import { alarmsFor } from '$lib/components/judges/judges';

	const TURN = alarmsFor(ALARMS, BUNDLES, 'turn');
	const onAcknowledge = fn(async () => {});

	const { Story } = defineMeta({
		title: 'Settings/EvalAlarmList',
		component: EvalAlarmList,
		tags: ['autodocs']
	});
</script>

<Story
	name="Turn judge alarms"
	args={{ alarms: TURN, onAcknowledge }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { name: 'Needs attention · 2' })).toBeInTheDocument();
		await expect(canvasElement.textContent).toContain('122 finished turns have not been judged.');
		await expect(canvasElement.textContent).toContain(
			'Last 7 days: 27% of turns labelled Corrected'
		);
		await expect(canvasElement.textContent).not.toMatch(/unjudged_backlog|turn_judge|shift:/);
		await userEvent.click(canvas.getAllByRole('button', { name: 'Acknowledge' })[0] as HTMLElement);
		await userEvent.type(canvas.getByRole('textbox'), 'backlog from the outage, draining');
		await userEvent.click(canvas.getByRole('button', { name: 'Save acknowledgement' }));
		await expect(onAcknowledge).toHaveBeenCalledWith(2713, 'backlog from the outage, draining');
	}}
/>
