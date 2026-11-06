<script lang="ts">
	import { Info } from '@lucide/svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import LessonRelationCard from './LessonRelationCard.svelte';
	import type { LessonRelation } from '$lib/api/memoryFeedback';

	interface Props {
		relations: LessonRelation[];
		total: number;
		onConfirm?: (relation: LessonRelation) => void;
		onReject?: (relation: LessonRelation) => void;
		decidingId?: number | null;
		decideErrors?: Record<number, string>;
		remaining?: number;
		loadingMore?: boolean;
		loadMoreError?: string | null;
		onLoadMore?: () => void;
		decisionAnnouncement?: string;
	}

	let {
		relations,
		total,
		onConfirm,
		onReject,
		decidingId = null,
		decisionAnnouncement = '',
		decideErrors = {},
		remaining = 0,
		loadingMore = false,
		loadMoreError = null,
		onLoadMore
	}: Props = $props();
</script>

<section
	class="rls-section"
	data-design="funk"
	data-augmented-ui="tl-clip br-clip border"
	aria-labelledby="rls-heading"
>
	<header class="rls-head">
		<h2 id="rls-heading" class="rls-title">Relationships</h2>
		{#if total > 0}
			<span class="rls-count">{total}</span>
		{/if}
		<Tooltip
			text="Lessons the consolidation pass has flagged as related to one another — one superseding, refining, recurring with, or contradicting another. Confirming or rejecting a relation never deletes a lesson; it only records the judgment."
		>
			<Info size={13} aria-hidden="true" class="rls-help-icon" />
		</Tooltip>
	</header>
	<span class="sr-only" role="status" aria-live="polite">{decisionAnnouncement}</span>

	{#if relations.length === 0}
		<p class="rls-empty">No lesson relationships have been proposed yet.</p>
	{:else}
		<ul class="rls-list" role="list">
			{#each relations as relation (relation.id)}
				<li>
					<LessonRelationCard
						{relation}
						{onConfirm}
						{onReject}
						deciding={decidingId === relation.id}
						decideError={decideErrors[relation.id] ?? null}
					/>
				</li>
			{/each}
		</ul>
		{#if remaining > 0}
			<div class="rls-footer">
				<span class="rls-footer__count" role="status">
					Showing {relations.length} of {total.toLocaleString()}; {remaining.toLocaleString()} more not
					loaded.
				</span>
				<button
					type="button"
					class="rls-footer__load-more"
					disabled={loadingMore}
					onclick={onLoadMore}
				>
					{loadingMore ? 'Loading…' : 'Load more relationships'}
				</button>
				{#if loadMoreError}
					<span class="rls-footer__error" role="alert">{loadMoreError}</span>
				{/if}
			</div>
		{/if}
	{/if}
</section>

<style>
	.rls-section {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--edge-line);
		background: var(--edge-surface);
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--edge-pad, calc(var(--edge) + var(--space-1)));
	}

	.rls-head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.rls-title {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--text-1);
	}

	.rls-count {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--text-2);
	}

	:global(.rls-help-icon) {
		margin-left: auto;
		color: var(--text-2);
	}

	.rls-empty {
		margin: 0;
		font-size: var(--text-sm);
		font-style: italic;
		color: var(--text-2);
	}

	.rls-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.rls-footer {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}
	.rls-footer__count {
		font-size: var(--text-label);
		color: var(--text-2);
	}
	.rls-footer__load-more {
		border: 1px solid var(--border-default);
		background: var(--surface-2);
		color: var(--text-1);
		border-radius: 4px;
		padding: 4px 10px;
		font-size: var(--text-label);
		cursor: pointer;
	}
	.rls-footer__load-more:hover:not(:disabled) {
		border-color: var(--interactive-active-border);
		color: var(--text-strong);
	}
	.rls-footer__load-more:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.rls-footer__error {
		font-size: var(--text-label);
		color: var(--status-danger-fg);
	}
</style>
