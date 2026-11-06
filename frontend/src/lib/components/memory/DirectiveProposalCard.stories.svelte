<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import DirectiveProposalCard from './DirectiveProposalCard.svelte';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import type { DirectiveProposal } from '$lib/api/directiveProposals';
	import type { DirectiveProposalApprovedDto, DirectiveProposalRejectedDto } from '$lib/types';

	const { Story } = defineMeta({
		title: 'Memory/DirectiveProposalCard',
		component: DirectiveProposalCard,
		tags: ['autodocs']
	});

	const PROPOSAL: DirectiveProposal = {
		id: 7,
		uuid: '00000000-0000-4000-8000-000000000007',
		source: 'confirmed_handback',
		scope: 'project',
		project_id: 2,
		class: 'push',
		polarity: 'allow',
		status: 'open',
		proposed_kind: 'convention',
		proposed_text:
			'You may push in /work/mike without asking first. You were authorised for this in 3 sessions:\nsession 1, turn 4: ok to push',
		evidence_count: 3,
		session_count: 3,
		first_evidence_at: '2026-09-10T10:00:00Z',
		last_evidence_at: '2026-09-20T10:00:00Z',
		created_at: '2026-09-21T03:00:00Z',
		decided_at: null,
		decision_note: null
	};

	interface Sent {
		method: string;
		url: string;
		body: unknown;
	}
	let sent: Sent[] = [];

	function router(method: string, url: string, body: unknown): TypedBody {
		sent.push({ method, url, body });
		if (method === 'POST' && url.endsWith('/api/directive-proposals/7/approve')) {
			return typed<DirectiveProposalApprovedDto>({ id: 7, status: 'approved', lessons: [] });
		}
		if (method === 'POST' && url.endsWith('/api/directive-proposals/7/reject')) {
			return typed<DirectiveProposalRejectedDto>({ id: 7, status: 'rejected' });
		}
		return typed<Record<string, never>>({});
	}
</script>

<Story
	name="DirectiveProposalCard"
	loaders={[
		async () => {
			sent = [];
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvasElement.textContent).toContain('You confirmed this was already authorised');
		await expect(canvasElement.querySelector('[data-proposal-text]')?.textContent).toBe(
			PROPOSAL.proposed_text
		);

		// Approve, with the text edited first — exact method, path and body land at the API.
		await userEvent.click(canvas.getByRole('button', { name: 'Approve' }));
		const textarea = canvas.getByLabelText('Approve this as a lesson (edit the text if you want):');
		await expect(textarea).toHaveValue(PROPOSAL.proposed_text);

		await userEvent.clear(textarea);
		await userEvent.type(textarea, 'Push freely in /work/mike.');

		await userEvent.click(canvas.getByRole('button', { name: 'Confirm approve' }));
		await waitFor(() => expect(sent).toHaveLength(1));
		expect(sent[0]).toEqual({
			method: 'POST',
			url: '/api/directive-proposals/7/approve',
			body: {
				text: 'Push freely in /work/mike.',
				kind: 'convention',
				target_project_ids: [2]
			}
		});

		// Reject, on the same card, with a note — back out of the still-open approve panel first
		// (a successful approve doesn't reset it locally; the real page removes the whole card
		// instead, via onDecided, which this isolated story doesn't wire).
		await userEvent.click(canvas.getByRole('button', { name: 'Cancel' }));
		const note = canvas.getByLabelText('Why not (optional)');
		await userEvent.type(note, 'Too broad for a standing rule.');
		await userEvent.click(canvas.getByRole('button', { name: 'Reject' }));

		await waitFor(() => expect(sent).toHaveLength(2));
		expect(sent[1]).toEqual({
			method: 'POST',
			url: '/api/directive-proposals/7/reject',
			body: { note: 'Too broad for a standing rule.' }
		});
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<DirectiveProposalCard proposal={PROPOSAL} />
		</div>
	{/snippet}
</Story>
