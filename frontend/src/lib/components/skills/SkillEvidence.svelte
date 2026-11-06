<!--
@component
The reviewer's evidence for one skill draft: the trigger test, the reviewer model's decision and
its reasons, the text before the reviewer rewrote it, and every trigger query with whether it
should and did load the skill.
-->
<script lang="ts">
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { percent, type SkillDraft, type TriggerTest } from '$lib/api/skills';
	import { DECISION_TEXT, reviewerSentence } from './skillEvidenceText';

	let { draft }: { draft: SkillDraft } = $props();

	const evaluation = $derived(draft.eval);
	const verdict = $derived(evaluation?.verdict);
	const trigger = $derived(evaluation?.trigger);
	const recheck = $derived(evaluation?.recheck ?? null);
	const review = $derived(evaluation?.review);
	const tests = $derived(
		[
			{ title: 'Would Claude load it?', test: trigger },
			{ title: 'Would Claude load the rewritten skill?', test: recheck }
		].filter((t): t is { title: string; test: TriggerTest } => !!t.test?.results.length)
	);

	const summary = $derived.by(() => {
		if (draft.status === 'draft') return 'The reviewer has not run on this draft yet.';
		if (draft.status === 'evaluating') return 'The reviewer is part way through this draft.';
		if (evaluation?.error) return `The review failed: ${evaluation.error}`;
		if (verdict?.accepted) return 'Passed review.';
		return 'Did not pass review.';
	});

	function score(test: TriggerTest | null | undefined): string {
		return test ? `${test.correct} of ${test.total} (${percent(test.score)})` : '—';
	}
</script>

<div class="sev" data-slot="skill-evidence">
	<p class="sev-summary">{summary}</p>

	{#if verdict}
		<dl class="sev-figures">
			<div>
				<dt>Trigger test</dt>
				<dd>{score(trigger)}, needs {percent(verdict.min_trigger_score)}</dd>
			</div>
			{#if recheck}
				<div>
					<dt>After the rewrite</dt>
					<dd>{score(recheck)}</dd>
				</div>
			{/if}
			<div>
				<dt>Reviewer</dt>
				<dd>{review ? DECISION_TEXT[review.decision] : 'not run'}</dd>
			</div>
			<div>
				<dt>Model</dt>
				<dd>{evaluation?.models?.join(', ') ?? draft.model}</dd>
			</div>
			{#if evaluation?.calls !== undefined}
				<div>
					<dt>Model calls</dt>
					<dd>{evaluation.calls}</dd>
				</div>
			{/if}
		</dl>
		{#if review}
			<p class="sev-review">{reviewerSentence(review)}</p>
		{/if}
		{#if verdict.reasons.length}
			<ul class="sev-reasons" aria-label="Why it did not pass">
				{#each verdict.reasons as reason (reason)}
					<li>{reason}</li>
				{/each}
			</ul>
		{/if}
	{/if}

	{#if review?.decision === 'patch' && review.original_skill_md}
		<SectionCard
			title="The text before the reviewer's rewrite"
			description={review.original_description ?? undefined}
		>
			<pre class="sev-original">{review.original_skill_md}</pre>
		</SectionCard>
	{/if}

	{#each tests as { title, test } (title)}
		<SectionCard
			{title}
			description="Each request was shown to a model beside the project's other skills. The skill should load for the first kind and stay unloaded for the second."
		>
			<table class="sev-table">
				<thead>
					<tr><th scope="col">Request</th><th scope="col">Should load</th><th scope="col">Loaded</th></tr>
				</thead>
				<tbody>
					{#each test.results as result, i (i)}
						<tr data-correct={result.correct}>
							<td>{result.query}</td>
							<td>{result.should_trigger ? 'yes' : 'no'}</td>
							<td>{result.triggered ? 'yes' : 'no'}{result.correct ? '' : ', wrong'}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</SectionCard>
	{/each}
</div>

<style>
	.sev {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.sev-summary,
	.sev-review {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.sev-figures {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-5);
		margin: 0;
		font-size: var(--text-xs);
	}
	.sev-figures dt {
		color: var(--text-3);
	}
	.sev-figures dd {
		margin: 0;
		color: var(--text-1);
	}
	.sev-reasons {
		margin: 0;
		padding-left: var(--space-4);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.sev-original {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.sev-table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-xs);
	}
	.sev-table th {
		text-align: left;
		color: var(--text-3);
		font-weight: 400;
		padding: var(--space-1) var(--space-2);
	}
	.sev-table td {
		padding: var(--space-1) var(--space-2);
		border-top: 1px solid var(--border-default);
		color: var(--text-1);
		vertical-align: top;
	}
	.sev-table tr[data-correct='false'] td {
		color: var(--status-danger-fg);
	}
</style>
