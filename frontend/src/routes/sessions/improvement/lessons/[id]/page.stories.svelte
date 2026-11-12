<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, expect, waitFor, within } from 'storybook/test';
	import LessonDetailPage from './+page.svelte';
	import type { LessonDetailPageData } from './+page';
	import { LESSON_DETAIL_RAW, LESSON_DETAIL_RETIRED } from '$lib/test-support/improvementFixtures';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';

	const DATA: LessonDetailPageData = {
		id: LESSON_DETAIL_RAW.id,
		lesson: LESSON_DETAIL_RAW,
		error: null
	};

	let approvedBody: unknown = null;

	function router(method: string, url: string, body: unknown): TypedBody {
		if (method === 'POST' && url.includes(`/lessons/${LESSON_DETAIL_RAW.id}/approve`)) {
			approvedBody = body;
			return typed<{ id: number; stage: string }>({ id: LESSON_DETAIL_RAW.id, stage: 'daily' });
		}
		return typed<Record<string, never>>({});
	}

	const { Story } = defineMeta({
		title: 'Routes/SessionsImprovementLesson',
		component: LessonDetailPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			approvedBody = null;
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			canvas.getByRole('heading', { level: 1, name: 'Lesson waiting on you' })
		).toBeInTheDocument();
		await expect(canvas.getByText(/Read where it came from, then approve it/)).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: 'What the judge did' })).toHaveAttribute(
			'href',
			'/sessions/improvement'
		);

		// The spine: where it came from, each quote framed by who said it.
		await expect(
			canvas.getByRole('heading', { level: 2, name: 'Where it came from' })
		).toBeInTheDocument();
		await expect(canvas.getByText(/Correction · proposed by the judge/)).toBeInTheDocument();
		await expect(canvas.getByText('You asked')).toBeInTheDocument();
		await expect(canvas.getByText('How the turn ended')).toBeInTheDocument();
		await expect(canvas.getByText('Your correction')).toBeInTheDocument();
		await expect(canvas.getByText(/This has been addressed i think before/)).toBeInTheDocument();

		// The judge's rationale is a collapsed band: hidden until opened.
		await expect(canvas.queryByText(/must be incorporated/)).not.toBeInTheDocument();
		const why = canvas.getByRole('button', { name: /Why the judge called this a correction/ });
		await expect(why).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(why);
		await expect(why).toHaveAttribute('aria-expanded', 'true');
		await expect(await canvas.findByText(/must be incorporated/)).toBeInTheDocument();

		// Similar earlier corrections: open by default.
		await expect(
			canvas.getByRole('button', { name: /Similar earlier corrections/ })
		).toHaveAttribute('aria-expanded', 'true');
		await expect(canvas.getByText('Fix the capture-DB retry loop')).toBeInTheDocument();

		// Not yet live: no "Since it went live" band.
		await expect(canvas.queryByText(/Since it went live/)).not.toBeInTheDocument();

		// The rail: editable while raw; its reset row is reserved but hidden until the text changes.
		const textarea = canvas.getByRole('textbox', { name: 'The rule' });
		await expect(textarea).toHaveValue('Leave the code better than you found it.');
		const reset = canvas.getByText("Reset to the judge's wording");
		await expect(reset).not.toBeVisible();
		const decide = canvas.getByRole('heading', { name: 'Decide' });
		const decideTop = decide.getBoundingClientRect().top;

		await userEvent.clear(textarea);
		await userEvent.type(textarea, 'Leave the code better than you found it, always.');
		await expect(reset).toBeVisible();
		await expect(Math.abs(decide.getBoundingClientRect().top - decideTop)).toBeLessThan(2);
		await expect(canvas.getByTestId('lesson-action-effect')).toBeInTheDocument();

		// Reject shows the reasons; rejecting again cancels them.
		const rejectButton = canvas.getByRole('button', { name: 'Reject' });
		await userEvent.click(rejectButton);
		await expect(rejectButton).toHaveAttribute('aria-expanded', 'true');
		const reasons = canvas.getByRole('group', { name: 'Why reject it?' });
		await expect(
			within(reasons).getByRole('button', { name: 'Too specific to be a general rule' })
		).toBeInTheDocument();
		await userEvent.click(rejectButton);
		await expect(canvas.queryByRole('group', { name: 'Why reject it?' })).not.toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: 'Approve' }));
		await waitFor(() =>
			expect(approvedBody).toMatchObject({
				text: 'Leave the code better than you found it, always.'
			})
		);
		await expect(await canvas.findByText('Saved. You can leave this page.')).toBeInTheDocument();
		await expect(
			canvas.getByRole('heading', { level: 1, name: 'Approved lesson' })
		).toBeInTheDocument();
	}}
/>

<Story
	name="Retired"
	loaders={[
		async () => {
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	args={{
		data: { id: LESSON_DETAIL_RETIRED.id, lesson: LESSON_DETAIL_RETIRED, error: null }
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			canvas.getByRole('heading', { level: 2, name: 'Since it went live' })
		).toBeInTheDocument();
		const outcomes = canvas.getByTestId('lesson-outcomes');
		await expect(outcomes).toHaveTextContent(
			'Judged in 12 sessions: followed in 1, corrected anyway in 2, ignored in 9.'
		);
		const retired = canvas.getByTestId('lesson-retired');
		await expect(retired).toHaveTextContent(/Superseded: no longer supplied\./);
		await expect(retired).toHaveTextContent(/Contradicted by the newer lesson 68/);
		await expect(retired).toHaveTextContent(/Search still finds it\./);
		await expect(within(retired).getByRole('link', { name: 'Read the newer lesson' })).toHaveAttribute(
			'href',
			'/sessions/improvement/lessons/68'
		);
	}}
/>
