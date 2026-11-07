<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import DirectiveProposalsSection from './DirectiveProposalsSection.svelte';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import type { DirectiveProposal, ProposalDetail } from '$lib/api/directiveProposals';
	import type { DirectiveProposalApprovedDto, DirectiveProposalRejectedDto } from '$lib/types';

	let rejected = false;

	const PROPOSAL: DirectiveProposal = {
		id: 4,
		uuid: '00000000-0000-4000-8000-000000000004',
		source: 'repeated_directive',
		scope: 'project',
		project_id: 2,
		class: 'commit',
		polarity: 'allow',
		status: 'open',
		proposed_kind: 'preference',
		proposed_text: 'Just commit when the tests are green, stop asking me every time.',
		evidence_count: 3,
		session_count: 2,
		first_evidence_at: '2026-09-10T10:00:00Z',
		last_evidence_at: '2026-09-20T10:00:00Z',
		created_at: '2026-09-21T03:00:00Z',
		decided_at: null,
		decision_note: null
	};

	function router(method: string, url: string): TypedBody {
		if (method === 'POST' && url.includes('/approve')) {
			rejected = true; // reuses the "decided" flag: an approved proposal also drops off the list
			return typed<DirectiveProposalApprovedDto>({ id: 4, status: 'approved', lessons: [] });
		}
		if (method === 'POST' && url.includes('/reject')) {
			rejected = true;
			return typed<DirectiveProposalRejectedDto>({ id: 4, status: 'rejected' });
		}
		if (url.match(/\/api\/directive-proposals\/4$/)) {
			return typed<ProposalDetail>({
				proposal: PROPOSAL,
				evidence: [
					{
						kind: 'dossier_entry',
						ref_id: 11,
						operator_turn_id: 90,
						session_id: 1,
						observed_at: PROPOSAL.first_evidence_at,
						detail: {},
						message_text: 'just commit it',
						gone: false
					},
					{
						kind: 'dossier_entry',
						ref_id: 12,
						operator_turn_id: null,
						session_id: 2,
						observed_at: PROPOSAL.last_evidence_at,
						detail: {},
						message_text: null,
						gone: true
					}
				]
			});
		}
		if (url.includes('/api/directive-proposals')) {
			return typed<{ proposals: DirectiveProposal[]; next_cursor: number | null }>({
				proposals: rejected ? [] : [PROPOSAL],
				next_cursor: null
			});
		}
		return typed<Record<string, never>>({});
	}

	const { Story } = defineMeta({
		title: 'Memory/DirectiveProposalsSection',
		component: DirectiveProposalsSection,
		tags: ['autodocs']
	});
</script>

<Story
	name="DirectiveProposalsSection"
	loaders={[
		async () => {
			rejected = false;
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-proposal-id="4"]')).not.toBeNull()
		);
		await expect(canvasElement.querySelector('[data-proposal-text]')?.textContent).toBe(
			PROPOSAL.proposed_text
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Every piece of evidence' }));
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-evidence-ref="12"]')?.textContent).toContain(
				'is gone'
			)
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Reject' }));
		await waitFor(() => expect(canvas.getByText('No open proposals.')).toBeTruthy());
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<DirectiveProposalsSection />
		</div>
	{/snippet}
</Story>
