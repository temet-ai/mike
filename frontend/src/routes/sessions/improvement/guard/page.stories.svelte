<!--
/sessions/improvement/guard ("Guard nudges"): a summary strip, a vertical list of questions
(`?question=`), the open question's GuardQuestionPanel card and then its nudges. Rebuilt
against the real landed API — see GuardQuestionPanel.svelte's own doc comment.
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import GuardPage from './+page.svelte';
	import { GUARD_WAKES, IMPROVEMENT_SUMMARY } from '$lib/test-support/improvementFixtures';
	import type { GuardPageData, GuardPageQuestion } from './+page';
	import type { GuardEffectEstimate } from '$lib/api/guardEffect';

	const NOT_ENOUGH_DATA: GuardEffectEstimate = {
		questionKey: 'unobserved_done',
		asOfSecs: 1790600000,
		primary: {
			treated: { n: 21, bad: 3, pBad: 0.143 },
			holdback: { n: 7, bad: 2, pBad: 0.286 },
			rd: null,
			ci: null
		},
		secondary: {
			treated: { n: 21, bad: 3, pBad: 0.143 },
			holdback: { n: 7, bad: 2, pBad: 0.286 },
			rd: null,
			ci: null
		},
		perProtocolNotRandomised: {
			treated: { n: 21, bad: 3, pBad: 0.143 },
			holdback: { n: 7, bad: 2, pBad: 0.286 },
			rd: null,
			ci: null
		},
		missing: {},
		events: 28,
		status: 'monitoring'
	};

	const MEASURED: GuardEffectEstimate = {
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

	const QUESTIONS: GuardPageQuestion[] = [
		{
			key: 'unobserved_done',
			mode: 'wake',
			counts24h: { rows: 28, wouldFire: 7 },
			effect: NOT_ENOUGH_DATA
		},
		{
			key: 'relapse',
			mode: 'wake',
			counts24h: { rows: 40, wouldFire: 9 },
			effect: MEASURED
		}
	];

	const DATA: GuardPageData = {
		readPointHoldback: 34,
		questions: QUESTIONS,
		questionsLoadError: null,
		wakesByQuestion: {
			unobserved_done: [{ ...GUARD_WAKES[0]! }, { ...GUARD_WAKES[2]! }],
			relapse: [{ ...GUARD_WAKES[1]! }]
		},
		wakesLoadError: null,
		weekNudges: IMPROVEMENT_SUMMARY.week
	};

	const gotoSpy = fn();

	function json(body: unknown): Promise<Response> {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	interface Sent {
		method: string;
		url: string;
		body: unknown;
	}
	let sent: Sent[] = [];

	function fetchStub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
		const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
		const method = init?.method ?? 'GET';
		const body = init?.body ? JSON.parse(init.body as string) : undefined;
		sent.push({ method, url, body });
		if (url.includes('/label')) return json({ ok: true });
		return json({ error: 'unexpected fetch' });
	}

	const { Story } = defineMeta({
		title: 'Routes/SessionsImprovementGuard',
		component: GuardPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			sent = [];
			gotoSpy.mockClear();
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: DATA }}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/sessions/improvement/guard?question=relapse'),
					params: {}
				}
			},
			navigation: { goto: gotoSpy }
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(
			canvas.getByRole('heading', { level: 1, name: 'Guard nudges' })
		).toBeInTheDocument();
		await expect(canvas.getByText(/Mark each nudge right or wrong\./)).toBeInTheDocument();

		const strip = within(canvasElement.querySelector('[data-slot="guard-strip"]')!);
		await expect(strip.getByText('Nudges sent, last 7 days')).toBeInTheDocument();
		await expect(strip.getByText('10')).toBeInTheDocument();
		await expect(strip.getByText('7 wakes · 3 notes')).toBeInTheDocument();
		const waitingLink = strip.getByRole('link', { name: /^Waiting for your answer: 2\./ });
		await expect(waitingLink).toHaveAttribute('href', '/sessions/improvement?waiting=nudges');

		// `?question=relapse` opens the second question; each carries a short name, its
		// delivered-nudge count and its mode, and the full question heads the open panel.
		const list = canvas.getByRole('tablist', { name: 'Guard questions' });
		await expect(list).toHaveAttribute('aria-orientation', 'vertical');
		const shipTab = canvas.getByRole('tab', { name: /^Done without checking/ });
		const relapseTab = canvas.getByRole('tab', { name: /^Repeats a correction/ });
		await expect(relapseTab).toHaveAttribute('aria-selected', 'true');
		await expect(shipTab).toHaveTextContent(/^Done without checking 2 Live/);
		await expect(relapseTab).toHaveTextContent(/^Repeats a correction 1 Live, starts a new turn$/);

		// The question card states what the question is; the nudges are in the list after it, not in it.
		const openPanel = canvas.getByRole('tabpanel');
		const panel = within(openPanel);
		await expect(
			panel.getByRole('heading', {
				level: 2,
				name: 'Does this reply repeat a correction you already made?'
			})
		).toBeInTheDocument();
		const rail = within(openPanel.querySelector('section.gqp')!);
		await expect(
			rail.getByRole('heading', { name: 'Live — starts a new turn' })
		).toBeInTheDocument();
		await expect(
			rail.getByText('This question is live — it can start a new turn for the model.')
		).toBeInTheDocument();
		await expect(rail.getByText('40 / 34')).toBeInTheDocument();
		await expect(
			rail.getByText(/Fewer corrections in the treated arm: 14% of 120 fires/)
		).toBeInTheDocument();
		await expect(rail.getByTestId('guard-marks-effect')).toBeVisible();
		await expect(rail.queryByTestId('guard-wake-list')).not.toBeInTheDocument();
		await expect(rail.queryByRole('button', { name: 'Right' })).not.toBeInTheDocument();
		await expect(panel.getAllByTestId('guard-marks-effect')).toHaveLength(1);
		await expect(panel.getAllByTestId('guard-wake-list')).toHaveLength(1);

		// Marking a nudge is a real POST to the evaluation's own uuid, and updates in place.
		await userEvent.click(panel.getByRole('button', { name: 'Right' }));
		await waitFor(() => expect(panel.getByText('You said right')).toBeInTheDocument());
		await waitFor(() => expect(sent).toHaveLength(2));
		expect(sent.map((s) => ({ method: s.method, url: s.url, body: s.body }))).toEqual([
			{
				method: 'POST',
				url: `/api/steering/evaluations/${GUARD_WAKES[1]!.evaluationUuid}/label`,
				body: { label: 'right' }
			},
			{
				method: 'POST',
				url: `/api/steering/evaluations/${GUARD_WAKES[1]!.repeatUuids![0]}/label`,
				body: { label: 'right' }
			}
		]);
		await expect(
			strip.getByRole('link', { name: /^Waiting for your answer: 1\./ })
		).toBeInTheDocument();

		await userEvent.click(shipTab);
		await expect(shipTab).toHaveAttribute('aria-selected', 'true');
		await waitFor(() =>
			expect(gotoSpy).toHaveBeenCalledWith('/sessions/improvement/guard?question=unobserved_done', {
				replace: true,
				reset: false
			})
		);
		const shipPanel = within(canvas.getByRole('tabpanel'));
		await expect(
			shipPanel.getByText(/Not enough data yet \(7 of 34 holdback events collected so far\)\./)
		).toBeInTheDocument();
		await expect(shipPanel.getByText('Not ready to answer yet')).toBeVisible();
	}}
/>
