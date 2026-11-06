<script lang="ts">
	import { outcomeBucketTone, type LessonOutcomeSummary } from '$lib/api/memoryFeedback';

	interface Props {
		outcome: LessonOutcomeSummary;
	}

	let { outcome }: Props = $props();

	function bucketLabel(
		bucket: LessonOutcomeSummary['outcomes_observed_after_injection'][number]
	): string {
		const quality =
			bucket.avg_quality_score !== null
				? ` (avg quality ${bucket.avg_quality_score.toFixed(2)})`
				: '';
		return `${bucket.outcome} ${bucket.count}${quality}`;
	}
</script>

<article
	class="lor-row"
	data-design="funk"
	data-augmented-ui="tl-clip br-clip border"
	data-lesson-id={outcome.lesson_id}
>
	<header class="lor-head">
		<span class="lor-chip">lesson #{outcome.lesson_id}</span>
		<span class="lor-chip">{outcome.kind}</span>
		<span class="lor-chip">injected {outcome.injection_count}×</span>
		<span class="lor-chip">
			{outcome.distinct_session_count} distinct session{outcome.distinct_session_count === 1
				? ''
				: 's'}
		</span>
	</header>

	{#if outcome.lesson_text}
		<p class="lor-text">{outcome.lesson_text}</p>
	{:else}
		<p class="lor-text lor-text--missing">
			Full lesson text not returned by this endpoint yet — shown by kind and id only.
		</p>
	{/if}

	<div
		class="lor-outcomes"
		role="group"
		aria-label="Session-judge outcomes observed after injection"
	>
		{#each outcome.outcomes_observed_after_injection as bucket (bucket.outcome)}
			<span class="badge-nx badge-{outcomeBucketTone(bucket.outcome)}">{bucketLabel(bucket)}</span>
		{/each}
	</div>
</article>

<style>
	.lor-row {
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

	.lor-head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		flex-wrap: wrap;
	}

	.lor-chip {
		display: inline-flex;
		align-items: center;
		border-radius: var(--r-tol);
		background: var(--surface-2);
		color: var(--text-2);
		padding: 2px var(--space-2);
		font-size: var(--text-label);
		font-family: var(--font-mono, monospace);
	}

	.lor-text {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: var(--text-xs);
		line-height: var(--leading-body);
		color: var(--text-1);
	}

	.lor-text--missing {
		font-style: italic;
		color: var(--text-faint);
	}

	.lor-outcomes {
		display: flex;
		gap: var(--space-1);
		flex-wrap: wrap;
	}
</style>
