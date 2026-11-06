<script lang="ts">
	import type { LessonOutcomeSummary } from '$lib/api/memoryFeedback';
	import LessonOutcomeRow from './LessonOutcomeRow.svelte';

	interface Props {
		outcomes: LessonOutcomeSummary[];
	}

	let { outcomes }: Props = $props();
</script>

<section
	class="ocs-section"
	data-design="funk"
	data-augmented-ui="tl-clip br-clip border"
	aria-labelledby="ocs-heading"
>
	<header class="ocs-head">
		<h2 id="ocs-heading" class="ocs-title">Outcomes</h2>
		{#if outcomes.length > 0}
			<span class="ocs-count">{outcomes.length}</span>
		{/if}
	</header>
	<p class="ocs-caveat">
		What happened in sessions <em>after</em> each lesson was injected — not proof the lesson caused it.
		A lesson injected into a session that later succeeded is a correlation, not evidence of that outcome.
	</p>

	{#if outcomes.length === 0}
		<p class="ocs-empty">No lesson has been injected into a request yet.</p>
	{:else}
		<ul class="ocs-list" role="list">
			{#each outcomes as outcome (outcome.lesson_id)}
				<li><LessonOutcomeRow {outcome} /></li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.ocs-section {
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

	.ocs-head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.ocs-title {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--text-1);
	}

	.ocs-count {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--text-2);
	}

	.ocs-caveat {
		margin: 0;
		font-size: var(--text-xs);
		line-height: var(--leading-body);
		color: var(--text-2);
		font-style: italic;
	}

	.ocs-empty {
		margin: 0;
		font-size: var(--text-sm);
		font-style: italic;
		color: var(--text-2);
	}

	.ocs-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}
</style>
