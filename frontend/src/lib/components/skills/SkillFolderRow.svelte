<!--
@component
One learned skill folder of a project: its name, state, the description Claude sees, where it
is written on each machine, the lessons it came from, the reviewer's verdict, how it has done in
real use, and why mike removed it when it did.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import SkillStateBadge from './SkillStateBadge.svelte';
	import AcceptChance from '$lib/components/shared/AcceptChance.svelte';
	import { formatDay } from '$lib/components/eval/evalReview';
	import { percent, type SkillFolder } from '$lib/api/skills';
	import SkillPlacementAdvice from './SkillPlacementAdvice.svelte';
	import {
		DECISION_TEXT,
		installTextLabel,
		placementAdvice,
		retirementSentence,
		reviewerSentence
	} from './skillEvidenceText';

	let {
		skill,
		chance = null,
		showChance = false
	}: { skill: SkillFolder; chance?: number | null; showChance?: boolean } = $props();

	const href = $derived(
		skill.draft_id === null ? null : resolve(`skills/${skill.draft_id}` as `skills/${string}`)
	);

	const trigger = $derived.by(() => {
		if (skill.trigger_score === null) return 'not run';
		const first = percent(skill.trigger_score);
		return skill.patched_trigger_score === null
			? first
			: `${first}, ${percent(skill.patched_trigger_score)} after the rewrite`;
	});

	const use = $derived.by(() => {
		const live = skill.live_use;
		if (live === null) return skill.first_written === null ? 'not shipped' : 'not measured yet';
		return `${live.loads} loads, ${live.corrected_turns} of ${live.judged_turns} judged turns corrected`;
	});

	const retirement = $derived(retirementSentence(skill.live_use));
	const advice = $derived(skill.reviewer ? placementAdvice(skill.reviewer) : null);
	const showVerdict = $derived(
		skill.reviewer !== null &&
			advice === null &&
			skill.state !== 'failed_review' &&
			skill.state !== 'shipped' &&
			retirement === null
	);

	function removedText(reason: string | null, at: string | null): string {
		const day = formatDay(at) ?? '';
		if (reason === 'user') return `removed ${day}, you retired it`;
		if (reason === 'policy') return `removed ${day}, waiting for your approval`;
		if (reason === 'unloaded') return `removed ${day}, not loaded for 30 days`;
		if (reason === 'corrected_above_base') return `removed ${day}, it was not helping`;
		return `removed ${day}`;
	}
</script>

<article class="sfr" data-slot="skill-folder-row" data-state={skill.state}>
	<header class="sfr-head">
		<h3 class="sfr-name">
			{#if href}
				<a {href}>{skill.folder}</a>
			{:else}
				{skill.folder}
			{/if}
		</h3>
		<SkillStateBadge state={skill.state} />
		<span class="sfr-chance"><AcceptChance {chance} show={showChance} /></span>
	</header>

	<p class="sfr-desc">
		{skill.description ??
			'No draft yet. When mike ships this folder it writes a plain list of the lessons on this topic.'}
	</p>

	<dl class="sfr-facts">
		<div>
			<dt>Lessons</dt>
			<dd>{skill.lesson_count ?? '—'}</dd>
		</div>
		<div>
			<dt>Trigger test</dt>
			<dd>{trigger}</dd>
		</div>
		<div>
			<dt>Reviewer</dt>
			<dd>{skill.reviewer ? DECISION_TEXT[skill.reviewer.decision] : 'not run'}</dd>
		</div>
		<div>
			<dt>In use</dt>
			<dd>{use}</dd>
		</div>
	</dl>

	{#if skill.reviewer && href && advice}
		<SkillPlacementAdvice review={skill.reviewer} variant="row" decideHref={href} />
	{/if}

	{#if retirement}
		<p class="sfr-note">{retirement}</p>
	{:else if showVerdict && skill.reviewer}
		<p class="sfr-note">{reviewerSentence(skill.reviewer)}</p>
	{/if}

	{#if skill.state === 'failed_review' && skill.verdict_reasons?.length}
		<ul class="sfr-reasons" aria-label="Why it did not pass review">
			{#each skill.verdict_reasons as reason (reason)}
				<li>{reason}</li>
			{/each}
		</ul>
	{/if}

	<div class="sfr-where">
		<span class="sfr-where-label">Where it lives</span>
		{#if skill.installs.length === 0}
			<span class="sfr-where-none">Not written to any checkout yet.</span>
		{:else}
			<ul>
				{#each skill.installs as install (install.machine_name + install.path)}
					{@const label = installTextLabel(install, skill.draft_id, install.retired_at !== null)}
					<li>
						<span class="sfr-machine">{install.machine_name}</span>
						<code>{install.path}</code>
						<span class="sfr-holds" data-quiet={label.quiet}>{label.text}</span>
						{#if install.retired_at}
							<span class="sfr-removed"
								>{removedText(install.retired_reason, install.retired_at)}</span
							>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</article>

<style>
	.sfr {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		background: var(--surface-card);
		border: 1px solid var(--border-default);
	}
	.sfr-head {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		flex-wrap: wrap;
	}
	.sfr-chance {
		margin-left: auto;
	}
	.sfr-name {
		margin: 0;
		font-size: var(--text-base);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.sfr-name a {
		color: inherit;
	}
	.sfr-desc {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.sfr-facts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-5);
		margin: 0;
		font-size: var(--text-xs);
	}
	.sfr-facts dt {
		color: var(--text-3);
	}
	.sfr-facts dd {
		margin: 0;
		color: var(--text-1);
	}
	.sfr-note {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.sfr-reasons {
		margin: 0;
		padding-left: var(--space-4);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.sfr-where {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-xs);
	}
	.sfr-where ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.sfr-where li {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.sfr-where-label,
	.sfr-machine,
	.sfr-removed,
	.sfr-where-none {
		color: var(--text-3);
	}
	.sfr-holds {
		color: var(--text-1);
	}
	.sfr-holds[data-quiet='true'] {
		color: var(--text-3);
	}
	.sfr-where code {
		overflow-wrap: anywhere;
		color: var(--text-1);
	}
</style>
