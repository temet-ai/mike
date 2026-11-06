<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import type { CompressionReview, ReviewStatus } from '$lib/api/compressionReview';
	import { compressionReviewFixture } from '$lib/test-support/compressionReviewFixtures';
	import Page from './+page.svelte';

	function buildReviews(): CompressionReview[] {
		return [
			compressionReviewFixture(),
			{
				...compressionReviewFixture(),
				id: 'fixture-request',
				target: { kind: 'request', request_id: 473364 },
				status: 'investigating'
			},
			{
				...compressionReviewFixture(),
				id: 'fixture-response',
				target: { kind: 'response', request_id: 473365 },
				status: 'evaluated'
			}
		];
	}

	let records: CompressionReview[] = [];
	let readStatus = 200;
	const fetchMock = fn(async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
		const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
		const path = new URL(url, 'http://localhost').pathname;
		const method = init?.method ?? (input instanceof Request ? input.method : 'GET');
		if (method === 'GET' && path === '/api/compression-review') {
			return Response.json(records, { status: readStatus });
		}
		const record = records.find(
			(item) => path === `/api/compression-review/${encodeURIComponent(item.id)}/review`
		);
		if (method === 'POST' && record) {
			const payload = JSON.parse(String(init?.body)) as {
				status: ReviewStatus;
				note: string;
				actor: string;
			};
			record.status = payload.status;
			record.updated_at = '2026-10-04T12:01:00Z';
			record.history.push({ ...payload, action: 'review', at: record.updated_at });
			return Response.json({ id: record.id, status: record.status });
		}
		// No request falls through to a live inspect server.
		return Response.json({ error: `Unexpected ${method} ${path}` }, { status: 404 });
	});

	const { Story } = defineMeta({
		title: 'Routes/CompressionReview',
		component: Page,
		tags: ['autodocs'],
		beforeEach: () => {
			records = buildReviews();
			readStatus = 200;
			fetchMock.mockClear();
			const previous = globalThis.fetch;
			globalThis.fetch = fetchMock;
			return () => {
				globalThis.fetch = previous;
			};
		}
	});
</script>

<Story
	name="ReviewQueue"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { name: 'Compression review' })).toBeVisible();
		await expect(canvas.getByText(/Flags nominate evidence for evaluation/)).toBeVisible();
		await waitFor(() => expect(canvas.getByText('3 flags')).toBeVisible());
		await expect(
			canvas
				.getAllByRole('link', { name: 'Original capture' })
				.map((link) => link.getAttribute('href'))
		).toEqual([
			'/api/requests/473363/body/request',
			'/api/requests/473364/body/request',
			'/api/requests/473365/body/response'
		]);

		const firstReview = within(canvas.getAllByRole('article')[0]!);
		await userEvent.click(firstReview.getByText('Evidence and history (1)'));
		await expect(firstReview.getByText('Test result needed panic details')).toBeVisible();
		await expect(firstReview.getByText('Follow-up result request 473366')).toBeVisible();
		await expect(firstReview.getByRole('button', { name: 'Save review' })).toBeDisabled();
		await userEvent.selectOptions(firstReview.getByLabelText('Review status'), 'investigating');
		const note = 'Compare the original panic details with the follow-up result.';
		await userEvent.type(firstReview.getByLabelText('Review note'), note);
		await userEvent.click(firstReview.getByRole('button', { name: 'Save review' }));
		await waitFor(() =>
			expect(fetchMock).toHaveBeenCalledWith('/api/compression-review/fixture-flag/review', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ status: 'investigating', note, actor: 'inspect-ui' })
			})
		);
		await waitFor(() => expect(canvas.getByText('Evidence and history (2)')).toBeVisible());
		await userEvent.click(canvas.getByText('Evidence and history (2)'));
		await expect(canvas.getByText(note)).toBeVisible();
		await expect(
			within(canvas.getAllByRole('article')[0]!).getByLabelText('Review note')
		).toHaveValue('');

		// Exercise empty and filtered states through the real route controls.
		await userEvent.selectOptions(canvas.getByLabelText('Filter status'), 'dismissed');
		await waitFor(() => expect(canvas.getByText(/No flags match this status/)).toBeVisible());
		await expect(canvas.queryAllByRole('article')).toHaveLength(0);
		await userEvent.selectOptions(canvas.getByLabelText('Filter status'), 'investigating');
		await waitFor(() => expect(canvas.getByText('2 flags')).toBeVisible());
		await userEvent.selectOptions(canvas.getByLabelText('Filter status'), '');
		await waitFor(() => expect(canvas.getByText('3 flags')).toBeVisible());

		// A failed refresh can recover without navigating away or losing the review.
		readStatus = 503;
		await userEvent.click(canvas.getByRole('button', { name: 'Refresh' }));
		await waitFor(() =>
			expect(canvas.getByRole('alert')).toHaveTextContent('Review queue unavailable')
		);
		await expect(canvas.getByRole('alert')).toHaveTextContent(
			'Could not save or load compression review (503).'
		);
		readStatus = 200;
		await userEvent.click(canvas.getByRole('button', { name: 'Retry' }));
		await waitFor(() => expect(canvas.getByText('3 flags')).toBeVisible());
		await expect(canvas.queryByRole('alert')).not.toBeInTheDocument();
		await expect(canvas.getByText('Evidence and history (2)')).toBeVisible();
	}}
/>
