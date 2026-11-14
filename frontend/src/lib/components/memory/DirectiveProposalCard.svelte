<!--
@component
One standing-directive proposal: an instruction the operator had to give more than once, or a
step he confirmed was already authorised. Approve (with the text editable first) turns it into
a lesson that reaches later sessions the same way a judge-sourced lesson does; Reject archives
it with an optional note. See PLAN.md §2.2 and design-spec.md §9. One plain `band-body` band —
`DirectiveProposalsSection` is the `.edge.mech-group` card, matching how `LessonCandidateCard`
sits inside `LessonCandidateList`, so this reads as one family with those cards inside the
"Waiting for you" grid.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import {
		approveProposal,
		getProposal,
		rejectProposal,
		sourceLabel
	} from '$lib/api/directiveProposals';
	import type { DirectiveProposal, ProposalDetail } from '$lib/api/directiveProposals';
	import { formatDate } from '$lib/components/eval/evalReview';
	import AcceptChance from '$lib/components/shared/AcceptChance.svelte';

	let {
		proposal,
		onDecided,
		chance = null,
		showChance = false
	}: {
		proposal: DirectiveProposal;
		onDecided?: () => void;
		chance?: number | null;
		showChance?: boolean;
	} = $props();

	const evidenceWindow = $derived.by(() => {
		const first = formatDate(proposal.first_evidence_at);
		const last = formatDate(proposal.last_evidence_at);
		if (!first || !last) return '';
		return first === last ? ` on ${first}` : ` from ${first} to ${last}`;
	});

	let detail = $state<ProposalDetail | null>(null);
	let note = $state('');
	let approving = $state(false);
	let approveText = $state(untrack(() => proposal.proposed_text));
	let busy = $state(false);
	let rowError = $state<string | null>(null);

	async function toggleEvidence(): Promise<void> {
		if (detail) {
			detail = null;
			return;
		}
		try {
			detail = await getProposal(proposal.id);
		} catch (e) {
			rowError = e instanceof Error ? e.message : String(e);
		}
	}

	function startApprove(): void {
		approveText = proposal.proposed_text;
		approving = true;
	}

	function cancelApprove(): void {
		approving = false;
	}

	async function confirmApprove(): Promise<void> {
		busy = true;
		rowError = null;
		try {
			await approveProposal(proposal.id, {
				text: approveText,
				kind: proposal.proposed_kind,
				target_project_ids: proposal.project_id !== null ? [proposal.project_id] : []
			});
			onDecided?.();
		} catch (e) {
			rowError = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	async function reject(): Promise<void> {
		busy = true;
		try {
			await rejectProposal(proposal.id, note);
			rowError = null;
			onDecided?.();
		} catch (e) {
			rowError = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}
</script>

<article
	class="dpc band-body"
	data-proposal-id={proposal.id}
	aria-label="Standing directive proposal"
>
	<div class="dpc-body">
		<div class="dpc-meta-row">
			<p class="dpc-meta">
				<span class="machine">{sourceLabel(proposal.source)}</span> — {proposal.evidence_count} times
				in
				{proposal.session_count} sessions{evidenceWindow}
			</p>
			<AcceptChance {chance} show={showChance} />
		</div>
		<p class="dpc-text" data-proposal-text>{proposal.proposed_text}</p>

		<button
			type="button"
			class="dpc-evidence-toggle"
			aria-expanded={detail !== null}
			onclick={() => void toggleEvidence()}>Every piece of evidence</button
		>
		{#if detail}
			<ol class="dpc-evidence">
				{#each detail.evidence as ev (ev.kind + ev.ref_id)}
					<li class="dpc-evidence-item" data-evidence-ref={ev.ref_id}>
						{ev.gone ? `${ev.kind} ${ev.ref_id} is gone` : (ev.message_text ?? '')}
					</li>
				{/each}
			</ol>
		{/if}

		{#if approving}
			<div class="dpc-approve">
				<label class="dpc-approve-label" for={`dpc-approve-${proposal.id}`}
					>Approve this as a lesson (edit the text if you want):</label
				>
				<textarea
					id={`dpc-approve-${proposal.id}`}
					class="dpc-approve-text"
					bind:value={approveText}
					rows={3}></textarea>
				<div class="dpc-actions">
					<Button size="sm" disabled={busy} onclick={() => void confirmApprove()}>
						{busy ? 'Saving' : 'Confirm approve'}
					</Button>
					<Button variant="outline" size="sm" disabled={busy} onclick={cancelApprove}>
						Cancel
					</Button>
				</div>
			</div>
		{:else}
			<div class="dpc-actions">
				<Button size="sm" onclick={startApprove}>Approve</Button>
				<input
					class="dpc-note"
					placeholder="Why not (optional)"
					aria-label="Why not (optional)"
					bind:value={note}
				/>
				<Button variant="outline" size="sm" disabled={busy} onclick={() => void reject()}>
					Reject
				</Button>
			</div>
		{/if}

		{#if rowError}
			<p class="dpc-error" role="alert">{rowError}</p>
		{/if}
	</div>
</article>

<style>
	.dpc {
		min-width: 0;
	}
	.dpc-meta-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
	}
	.dpc-meta {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.dpc-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.dpc-text {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-display);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.dpc-evidence-toggle {
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-xs);
		color: var(--text-faint);
		cursor: pointer;
	}
	.dpc-evidence-toggle:hover {
		color: var(--text-strong);
	}
	.dpc-evidence-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.dpc-evidence {
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.dpc-evidence-item {
		list-style: none;
		font-size: var(--text-xs);
		color: var(--text-2);
		white-space: pre-wrap;
	}
	.dpc-approve {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.dpc-approve-label {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.dpc-approve-text {
		width: 100%;
		padding: var(--space-2);
		border: var(--hairline) solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-1);
		font: inherit;
		font-size: var(--text-sm);
		resize: vertical;
	}
	.dpc-actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
	.dpc-note {
		min-width: 14rem;
		flex: 1 1 auto;
		padding: var(--space-1) var(--space-2);
		border: var(--hairline) solid var(--border-subtle);
		background: var(--surface-bg);
		color: var(--text-1);
		font: inherit;
		font-size: var(--text-xs);
	}
	.dpc-error {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
	}
</style>
