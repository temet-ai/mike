<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import PatternCandidatesPanel from './PatternCandidatesPanel.svelte';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import type { CandidateEntry, CandidatesResponse } from '$lib/api/semantic';
	import type { PatternCandidateNamedDto } from '$lib/types';

	let status = 'open';

	const CANDIDATE: CandidateEntry = {
		candidate: {
			id: 7,
			status: 'open',
			window_start: '2026-09-01T00:00:00Z',
			window_end: '2026-10-01T00:00:00Z',
			member_item_ids: [11, 12, 13, 14],
			member_count: 4,
			session_count: 3,
			medoid_item_id: 12,
			created_at: '2026-10-01T00:00:00Z',
			decided_at: null,
			decision_note: null,
			named_pattern: null
		},
		members: [
			{ item_id: 11, text: 'you asked me twice and I still ran the wrong suite' },
			{ item_id: 12, text: 'run the real suite, not the one you edited' },
			{ item_id: 13, text: 'that is the old test file again' },
			{ item_id: 14, status: 'gone' }
		]
	};

	function router(method: string, url: string): TypedBody {
		if (method === 'POST' && url.includes('/name')) {
			status = 'named';
			return typed<PatternCandidateNamedDto>({
				candidate_id: 7,
				status: 'named',
				sort_order: 1,
				anchor_ids: []
			});
		}
		if (url.includes('/api/semantic/patterns/candidates')) {
			return typed<CandidatesResponse>({
				candidates: status === 'open' ? [CANDIDATE] : [],
				total: status === 'open' ? 1 : 0,
				limit: 50,
				offset: 0
			});
		}
		return typed<Record<string, never>>({});
	}

	const { Story } = defineMeta({
		title: 'Memory/PatternCandidatesPanel',
		component: PatternCandidatesPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="PatternCandidatesPanel"
	loaders={[
		async () => {
			status = 'open';
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText(/4 corrections in 3 sessions/)).toBeInTheDocument());
		await expect(canvas.getByText('run the real suite, not the one you edited')).toBeInTheDocument();
		await userEvent.click(canvas.getByRole('button', { name: 'Every member' }));
		await expect(canvas.getByText('item 14 is gone')).toBeInTheDocument();
		await userEvent.click(canvas.getByRole('button', { name: 'Name' }));
		await expect(canvas.getByRole('alert')).toHaveTextContent(/3 to 64 lowercase/);
		await userEvent.type(canvas.getByLabelText('Pattern name for candidate 7'), 'ran-wrong-suite');
		await userEvent.type(canvas.getByLabelText('Description for candidate 7'), 'Ran a different test suite than the gate runs.');
		await userEvent.click(canvas.getByRole('button', { name: 'Name' }));
		await waitFor(() => expect(canvas.getByText('No open candidates.')).toBeInTheDocument());
	}}
/>
