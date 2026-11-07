<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import RunsPage from './+page.svelte';
	import type { EvalRunsPageData } from './+page';
	import {
		BUNDLES,
		GATES,
		RUN_2_SESSION,
		TURN_RUNS,
		WORDING_BANNED,
		textOutsideCode
	} from '$lib/test-support/judgesFixtures';

	const DATA: EvalRunsPageData = {
		turnRuns: TURN_RUNS,
		sessionRuns: [RUN_2_SESSION],
		bundles: BUNDLES,
		gates: GATES,
		liveTurn: 14,
		liveSession: 13,
		tab: 'turn_judge',
		error: null
	};

	function json(body: unknown) {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	function fetchStub(input: RequestInfo | URL): Promise<Response> {
		const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
		if (url.includes('/api/eval/runs/') && url.includes('/items'))
			return json({ items: [], total: 40, reviewed: 0, limit: 1, offset: 0 });
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Sessions/LabellingAccuracy',
		component: RunsPage,
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);

		await expect(c.getByRole('heading', { level: 1, name: 'Judge accuracy' })).toBeInTheDocument();
		const strip = within(c.getByTestId('accuracy-strip'));
		await expect(c.getByTestId('accuracy-strip').textContent).toContain('Accuracy runs');
		const configs = strip.getAllByTestId('strip-config');
		await expect(configs.map((el) => el.querySelector('.page-strip__num')?.textContent)).toEqual([
			'14',
			'13'
		]);
		const configText = configs.map((el) => el.textContent ?? '');
		await expect(configText[0]).toContain('google/gemini-3.8-flash');
		await expect(configText[1]).toContain('google/gemini-3.5-flash-lite');
		const agreementText = strip.getAllByTestId('strip-agreement').map((el) => el.textContent ?? '');
		await expect(agreementText[0]).toContain('82%');
		await expect(agreementText[0]).toContain('163 of 200 turns');
		await expect(agreementText[1]).toContain('81%');
		await expect(
			strip.getAllByRole('link', { name: 'See the run' }).map((a) => a.getAttribute('href'))
		).toEqual(['/sessions/labelling/accuracy/6', '/sessions/labelling/accuracy/6']);
		const how = c.getByRole('button', { name: 'How accuracy is measured' });
		await expect(how).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(how);
		await expect(how).toHaveAttribute('aria-expanded', 'true');
		await expect(canvasElement.textContent).toContain('not proof the judge is right');

		const cards = [...canvasElement.querySelectorAll('[data-run]')].map((el) =>
			el.getAttribute('data-run')
		);
		await expect(cards).toEqual(['6', '5', '4', '3', '1']);

		const run6 = canvasElement.querySelector('[data-run="6"]') as HTMLElement;
		await expect(run6.textContent).toContain('Repeat of');
		await expect(
			within(run6)
				.getAllByRole('link', {
					name: /^Labelling accuracy, .* configuration 14 \(gemini-3\.8-flash\)$/
				})
				.map((a) => a.getAttribute('href'))
		).toEqual(['/sessions/labelling/accuracy/6', '/sessions/labelling/accuracy/4']);
		await expect(canvasElement.textContent).not.toMatch(/\brun [0-9]/i);

		const run5 = canvasElement.querySelector('[data-run="5"]') as HTMLElement;
		await expect(run5.textContent).toContain('this configuration does not answer this question');

		await userEvent.click(c.getByRole('tab', { name: /Sessions/ }));
		const run2 = canvasElement.querySelector('[data-run="2"]') as HTMLElement;
		await expect(run2.textContent).toContain('agreed on 1 of 1');

		await expect(textOutsideCode(canvasElement)).not.toMatch(WORDING_BANNED);
	}}
/>
