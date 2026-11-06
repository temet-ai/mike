<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import RunPage from './+page.svelte';
	import type { TestRunPageData } from './+page';
	import {
		BUNDLES,
		GATES,
		RUN_5,
		TURN_RUNS,
		WORDING_BANNED,
		textOutsideCode
	} from '$lib/test-support/judgesFixtures';

	const DATA: TestRunPageData = {
		runId: 5,
		run: RUN_5,
		error: null,
		runs: TURN_RUNS,
		bundles: BUNDLES,
		gates: GATES,
		liveId: 14,
		counts: { reaction: { total: 83, reviewed: 0 } }
	};

	const { Story } = defineMeta({
		title: 'Routes/Sessions/LabellingAccuracyRun',
		component: RunPage,
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);

		await expect(
			c.getByRole('heading', { level: 1, name: /^Labelling accuracy, 28 Sep( 2026)? \d\d:\d\d$/ })
		).toBeInTheDocument();

		const comparison = c.getByTestId('comparison');
		await expect(
			within(comparison).getByRole('columnheader', {
				name: /In use: configuration 14, measured\s*28 Sep( 2026)? \d\d:\d\d/
			})
		).toBeInTheDocument();
		await expect(
			within(comparison).getByRole('link', { name: /^28 Sep( 2026)? \d\d:\d\d$/ })
		).toHaveAttribute('href', expect.stringMatching(/\/sessions\/labelling\/accuracy\/6$/));
		await expect(canvasElement.textContent).not.toMatch(/\brun [0-9]/i);

		const useLink = c.getByRole('link', {
			name: "Use this run's configuration"
		}) as HTMLAnchorElement;
		await expect(useLink.getAttribute('href')).toMatch(/\/sessions\/labelling\?edit=19$/);

		await expect(textOutsideCode(canvasElement)).not.toMatch(WORDING_BANNED);
	}}
/>
