<script lang="ts">
	import { Check, X } from '@lucide/svelte';
	import {
		describeRelationKind,
		describeRelationMethod,
		relationStatusTone,
		isDecidableRelation,
		type LessonRelation
	} from '$lib/api/memoryFeedback';
	import { formatRelative, formatClockAbsolute } from '$lib/time';

	interface Props {
		relation: LessonRelation;
		onConfirm?: (relation: LessonRelation) => void;
		onReject?: (relation: LessonRelation) => void;
		deciding?: boolean;
		decideError?: string | null;
	}

	let { relation, onConfirm, onReject, deciding = false, decideError = null }: Props = $props();

	const kindLabel = $derived(describeRelationKind(relation.kind));
	const canDecide = $derived(isDecidableRelation(relation.status) && (!!onConfirm || !!onReject));
	const evidenceEntries = $derived(Object.entries(relation.evidence_json ?? {}));
	const evidenceJsonText = $derived(
		evidenceEntries.length > 0 ? JSON.stringify(relation.evidence_json, null, 2) : null
	);

	function handleConfirm(): void {
		onConfirm?.(relation);
	}
	function handleReject(): void {
		onReject?.(relation);
	}
</script>

<article
	class="lrc-card"
	data-design="funk"
	data-status={relation.status}
	data-augmented-ui="tl-clip br-clip border"
	data-relation-id={relation.id}
>
	<header class="lrc-head">
		<span class="badge-nx badge-muted">{kindLabel}</span>
		<span class="badge-nx badge-{relationStatusTone(relation.status)}">{relation.status}</span>
		<span class="lrc-chip">{describeRelationMethod(relation.method)}</span>
		<span
			class="lrc-when font-mono text-micro tabular-nums text-[var(--text-faint)]"
			title={formatClockAbsolute(relation.decided_at ?? relation.proposed_at)}
		>
			{relation.decided_at
				? `decided ${formatRelative(relation.decided_at)}`
				: `proposed ${formatRelative(relation.proposed_at)}`}
		</span>
		{#if canDecide}
			<div class="lrc-actions">
				<button type="button" class="lrc-confirm" disabled={deciding} onclick={handleConfirm}>
					<Check size={12} aria-hidden="true" />
					{deciding ? 'Working…' : 'Confirm'}
				</button>
				<button type="button" class="lrc-reject" disabled={deciding} onclick={handleReject}>
					<X size={12} aria-hidden="true" />
					{deciding ? 'Working…' : 'Reject'}
				</button>
			</div>
		{/if}
	</header>

	{#if decideError}
		<p class="lrc-decide-error" role="alert">Couldn't record that decision: {decideError}</p>
	{/if}

	<p class="lrc-summary">
		Lesson #{relation.from_lesson_id}
		<strong>{kindLabel.toLowerCase()}</strong>
		lesson #{relation.to_lesson_id}.
	</p>

	<div class="lrc-compare-grid">
		<div class="lrc-compare-cell">
			<span class="lrc-compare-label">From — lesson #{relation.from_lesson_id}</span>
			<p class="lrc-compare-text">{relation.from_lesson_text}</p>
		</div>
		<div class="lrc-compare-cell">
			<span class="lrc-compare-label">To — lesson #{relation.to_lesson_id}</span>
			<p class="lrc-compare-text">{relation.to_lesson_text}</p>
		</div>
	</div>

	<div class="lrc-evidence">
		<span class="lrc-evidence__label">Evidence</span>
		<dl class="lrc-evidence-list">
			{#if relation.candidate_distance !== null}
				<dt>Candidate distance</dt>
				<dd>
					{relation.candidate_distance.toFixed(4)}
					<span class="lrc-evidence__hint"
						>— nominated this pair for comparison only; it did not decide the kind or direction</span
					>
				</dd>
			{/if}
			{#if relation.proposer_model}
				<dt>Proposer</dt>
				<dd>{relation.proposer_model}</dd>
			{/if}
			{#if relation.proposer_rationale}
				<dt>Proposer rationale</dt>
				<dd class="lrc-rationale">{relation.proposer_rationale}</dd>
			{/if}
			{#if relation.adjudicator_model}
				<dt>Adjudicator</dt>
				<dd>
					{relation.adjudicator_model}
					{#if relation.adjudicator_agrees !== null}
						<span class="badge-nx badge-{relation.adjudicator_agrees ? 'active' : 'error'}">
							{relation.adjudicator_agrees ? 'agreed' : 'disagreed'}
						</span>
					{/if}
				</dd>
			{/if}
			{#if relation.adjudicator_rationale}
				<dt>Adjudicator rationale</dt>
				<dd class="lrc-rationale">{relation.adjudicator_rationale}</dd>
			{/if}
		</dl>
		{#if evidenceJsonText}
			<pre class="lrc-evidence-json">{evidenceJsonText}</pre>
		{/if}
	</div>
</article>

<style>
	.lrc-card {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--edge-line);
		background: var(--edge-surface);
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--edge-pad, calc(var(--edge) + var(--space-1)));
	}

	.lrc-head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		flex-wrap: wrap;
	}

	.lrc-when {
		margin-left: auto;
	}

	.lrc-chip {
		display: inline-flex;
		align-items: center;
		border-radius: 4px;
		background: var(--surface-2);
		color: var(--text-2);
		padding: 2px 6px;
		font-size: var(--text-label);
		font-family: var(--font-mono, monospace);
	}

	.lrc-card[data-status='proposed'] {
		--signal: var(--signal-info);
		--signal-edge: var(--blue-border);
	}
	.lrc-card[data-status='contested'] {
		--signal: var(--signal-warn);
		--signal-edge: var(--yellow-border);
	}
	.lrc-card[data-status='confirmed'] {
		--signal: var(--signal-complete);
		--signal-edge: var(--green-border);
	}
	.lrc-card[data-status='rejected'] {
		--signal: var(--signal-error);
		--signal-edge: var(--red-border);
	}
	.lrc-card[data-status] {
		--aug-border-bg: linear-gradient(135deg, var(--signal-edge), var(--line) 55%);
	}

	.lrc-actions {
		display: flex;
		gap: var(--space-2);
	}

	.lrc-confirm,
	.lrc-reject {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		border: var(--hairline) solid var(--line);
		background: var(--surface-2);
		color: var(--text-1);
		border-radius: var(--r-tol);
		padding: var(--space-1) var(--space-2);
		font-size: var(--text-xs);
		font-family: var(--font-mono, monospace);
		cursor: pointer;
		transition:
			color var(--dur-1) var(--ease-io),
			border-color var(--dur-1) var(--ease-io),
			box-shadow var(--dur-1) var(--ease-io);
	}
	.lrc-confirm:hover:not(:disabled) {
		border-color: var(--green-border);
		color: var(--signal-complete);
	}
	.lrc-reject:hover:not(:disabled) {
		border-color: var(--red-border);
		color: var(--signal-error);
	}
	.lrc-confirm:focus-visible,
	.lrc-reject:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}
	.lrc-confirm:disabled,
	.lrc-reject:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.lrc-confirm:focus-visible,
	.lrc-reject:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.lrc-decide-error {
		margin: 0;
		font-size: var(--text-label);
		color: var(--status-danger-fg);
	}

	.lrc-summary {
		margin: 0;
		font-size: 12px;
		color: var(--text-2);
	}
	.lrc-summary strong {
		color: var(--text-strong);
	}

	.lrc-compare-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1px;
		background: var(--border-default);
		border: 1px solid var(--border-default);
		border-radius: 4px;
		overflow: hidden;
	}
	.lrc-compare-cell {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px;
		background: var(--surface-card);
		min-width: 0;
	}
	.lrc-compare-label {
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-2);
	}
	.lrc-compare-text {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
	}

	.lrc-evidence {
		display: flex;
		flex-direction: column;
		gap: 6px;
		border-top: 1px solid var(--border-default);
		padding-top: 8px;
	}
	.lrc-evidence__label {
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-2);
	}
	.lrc-evidence__hint {
		color: var(--text-faint);
		font-style: italic;
	}

	.lrc-evidence-list {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 4px 8px;
		margin: 0;
		font-size: var(--text-label);
	}
	.lrc-evidence-list dt {
		color: var(--text-2);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.lrc-evidence-list dd {
		margin: 0;
		color: var(--text-1);
	}
	.lrc-rationale {
		white-space: pre-wrap;
		word-break: break-word;
		font-family: inherit;
	}

	.lrc-evidence-json {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--text-2);
		background: var(--surface-2);
		border-radius: 4px;
		padding: 6px 8px;
	}

	@media (max-width: 640px) {
		.lrc-compare-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
