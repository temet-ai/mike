<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import ImprovementPage from './+page.svelte';
	import type { ImprovementPageData } from './+page';
	import {
		ACTION_EFFECTS,
		DIRECTIVE_PROPOSALS,
		GUARD_WAKES,
		LESSON_CANDIDATES,
		IMPROVEMENT_SUMMARY,
		TREND_RESPONSE
	} from '$lib/test-support/improvementFixtures';
	import { REPEATS } from '$lib/test-support/repeatedAsksFixtures';

	const DATA: ImprovementPageData = {
		candidates: LESSON_CANDIDATES,
		candidatesError: null,
		proposals: DIRECTIVE_PROPOSALS,
		proposalsError: null,
		wakes: GUARD_WAKES,
		wakesError: null,
		summary: IMPROVEMENT_SUMMARY,
		summaryError: null,
		actions: ACTION_EFFECTS,
		actionsError: null,
		trend: TREND_RESPONSE,
		trendError: null,
		repeats: REPEATS,
		repeatsError: null
	};

	function json(body: unknown): Promise<Response> {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	function fetchStub(input: RequestInfo | URL): Promise<Response> {
		const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
		if (url.includes('/api/improvement/lessons/') && url.endsWith('/approve')) {
			return json({ ok: true });
		}
		if (url.includes('/api/directive-proposals')) {
			return json({ proposals: DIRECTIVE_PROPOSALS, next_cursor: null });
		}
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const gotoSpy = fn(async () => {});

	const { Story } = defineMeta({
		title: 'Routes/SessionsImprovement',
		component: ImprovementPage,
		tags: ['autodocs'],
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
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/sessions/improvement?waiting=nudges'),
					params: {}
				}
			},
			navigation: { goto: gotoSpy }
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			canvas.getByRole('heading', { level: 1, name: 'What the judge did' })
		).toBeInTheDocument();
		await expect(canvas.getByRole('heading', { name: 'Waiting for you' })).toBeInTheDocument();
		await expect(canvas.getByRole('heading', { name: 'Is it getting better' })).toBeInTheDocument();
		await expect(canvas.getByText(/not a controlled experiment/)).toBeInTheDocument();
		await expect(canvas.queryByText(/What happened/)).not.toBeInTheDocument();

		const strip = within(canvasElement.querySelector('[data-slot="improvement-strip"]')!);
		await expect(strip.getByRole('button', { name: /^Lessons: 4\./ })).toBeInTheDocument();
		await expect(strip.getByText('10 rejected')).toBeInTheDocument();
		await expect(strip.getByText('25%')).toBeInTheDocument();

		const lessonsTab = canvas.getByRole('tab', { name: /Lessons/ });
		const nudgesTab = canvas.getByRole('tab', { name: /Nudges/ });
		await expect(nudgesTab).toHaveAttribute('aria-selected', 'true');
		await expect(canvas.getByTestId('guard-marks-effect')).toBeVisible();
		await expect(canvas.getByText('Not ready to answer yet')).toBeVisible();

		await userEvent.click(lessonsTab);
		await expect(lessonsTab).toHaveAttribute('aria-selected', 'true');
		await waitFor(() =>
			expect(gotoSpy).toHaveBeenCalledWith('/sessions/improvement?waiting=lessons', {
				replaceState: true,
				noScroll: true,
				keepFocus: true
			})
		);
		await expect(lessonsTab).toHaveTextContent('Lessons4');
		const nudgesLeft = nudgesTab.getBoundingClientRect().left;

		const firstLesson = canvas.getAllByRole('article')[0]!;
		await userEvent.click(within(firstLesson).getByRole('button', { name: 'Approve' }));
		await waitFor(() => expect(within(firstLesson).getByText('Approved')).toBeInTheDocument());
		await expect(lessonsTab).toHaveTextContent('Lessons3');
		await expect(strip.getByRole('button', { name: /^Lessons: 3\./ })).toBeInTheDocument();
		await expect(nudgesTab.getBoundingClientRect().left).toBe(nudgesLeft);
		await expect(canvas.getAllByRole('article')[0]).toBe(firstLesson);

		await expect(canvas.getByText(/Fewer corrections:/)).toBeInTheDocument();
		await expect(
			canvas.getByText(/22–28 Sep: 25% of turns labelled Corrected \(71 of\s+283\)/)
		).toBeInTheDocument();
	}}
/>
