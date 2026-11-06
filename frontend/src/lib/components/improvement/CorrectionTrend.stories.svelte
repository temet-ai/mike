<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import CorrectionTrend from './CorrectionTrend.svelte';
	import { TREND_RESPONSE, TREND_RESPONSE_ONE_JUDGE } from '$lib/test-support/improvementFixtures';

	const { Story } = defineMeta({
		title: 'Improvement/CorrectionTrend',
		component: CorrectionTrend,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{
		weekly: TREND_RESPONSE.weekly,
		markers: TREND_RESPONSE.markers,
		windowLabel: 'last 8 weeks',
		error: null,
		onRetry: () => {}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			canvas.getByRole('group', { name: /weekly share of turns labelled corrected/i })
		).toBeInTheDocument();
		await expect(canvas.getByTestId('ct-scope')).toHaveTextContent(
			/every real turn the judge labelled in those seven days, in every project/
		);
		await expect(canvas.getByRole('link', { name: 'Judge page' })).toHaveAttribute(
			'href',
			'/sessions/labelling'
		);
		await expect(canvas.getByTestId('ct-latest')).toHaveTextContent(
			/22–28 Sep: 25% of turns labelled Corrected \(71 of\s*283\), up from 8\.4% in 15–21 Sep\./
		);
		const warn = canvas.getByTestId('ct-judge-changed');
		await expect(warn).toHaveTextContent(/The judge changed during these weeks/);
		await expect(warn).toHaveTextContent(/The latest week is one of them/);
		await expect(warn).toHaveTextContent(
			/1–7 Sep: google\/gemini-3-flash-preview began labelling; moonshotai\/kimi-k3 stopped\./
		);
		await expect(canvas.getAllByTestId('ct-change-line')).toHaveLength(3);
		await expect(canvas.getByText('30%')).toBeInTheDocument();
		await expect(canvas.getByText('28 Sep')).toBeInTheDocument();
		await expect(canvas.getByText('25%')).toBeInTheDocument();

		const weeks = canvas.getAllByTestId('ct-week');
		await expect(weeks).toHaveLength(8);
		weeks[6]!.focus();
		const tip = await canvas.findByTestId('ct-tooltip');
		await expect(tip).toHaveTextContent(/15–21 Sep/);
		await expect(tip).toHaveTextContent(/8\.4%\s*labelled Corrected/);
		await expect(tip).toHaveTextContent(/19 of 225 turns/);
		await expect(tip).toHaveTextContent(/configuration 14 began labelling/);
		await expect(tip).toHaveTextContent(/A lesson was approved/);
		weeks[6]!.blur();

		await userEvent.click(canvas.getByText('Show the numbers'));
		await expect(canvas.getByRole('cell', { name: '25% (71)' })).toBeVisible();
		await expect(canvas.getByRole('link', { name: /A lesson was approved/ })).toBeInTheDocument();
	}}
/>

<Story
	name="One judge throughout"
	args={{
		weekly: TREND_RESPONSE_ONE_JUDGE.weekly,
		markers: [],
		windowLabel: 'last 8 weeks',
		error: null,
		onRetry: () => {}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.queryByTestId('ct-judge-changed')).toBeNull();
		await expect(canvas.getByTestId('ct-latest')).toHaveTextContent(
			/22–28 Sep: 15% of turns labelled Corrected \(10 of\s*68\), down from 21% in 15–21 Sep\./
		);
	}}
/>

<Story
	name="No judged turns"
	args={{ weekly: [], markers: [], windowLabel: 'last 8 weeks', error: null, onRetry: () => {} }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText(/Not enough judged turns yet/)).toBeInTheDocument();
	}}
/>
