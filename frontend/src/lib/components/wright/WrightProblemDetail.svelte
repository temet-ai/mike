<script lang="ts">
	import { FileSearch, GitCommitVertical, GitPullRequest } from '@lucide/svelte';
	import type { Snippet } from 'svelte';
	import type { WrightProblem, WrightRepair } from '$lib/api/wright';
	import { Button } from '$lib/components/ui/button';
	import Badge from '$lib/components/shared/Badge.svelte';
	import LinkChipRow, { type LinkChip } from '$lib/components/shared/LinkChipRow.svelte';
	import type { Feed } from '$lib/wright/feed';
	import type { DetailFeed, ProblemDetailState, UndoStatus } from '$lib/wright/store.svelte';
	import {
		GITHUB_COMMIT,
		REPAIR_STOPPED,
		dayAndTime,
		findingEvidenceRef,
		humanise,
		repairSteps
	} from '$lib/wright/present';
	import WrightActionRow from './WrightActionRow.svelte';
	import WrightFacts from './WrightFacts.svelte';

	interface Props {
		problem: WrightProblem;
		detail: ProblemDetailState | undefined;
		undo: Record<string, UndoStatus>;
		undoDisabledReason?: string | null;
		onUndo: (actionId: number) => void;
		onMore: (feed: DetailFeed) => void;
		onRetry: () => void;
	}

	const {
		problem,
		detail,
		undo,
		undoDisabledReason = null,
		onUndo,
		onMore,
		onRetry
	}: Props = $props();

	function repairChips(r: WrightRepair): LinkChip[] {
		const chips: LinkChip[] = [];
		if (r.pr_url) {
			chips.push({
				icon: GitPullRequest,
				kind: 'PR',
				text: r.pr_url,
				href: r.pr_url,
				external: true
			});
		}
		if (r.merge_sha) {
			chips.push({
				icon: GitCommitVertical,
				kind: 'Commit',
				text: r.merge_sha,
				href: `${GITHUB_COMMIT}${r.merge_sha}`,
				external: true
			});
		}
		return chips;
	}
</script>

{#snippet note(feed: Feed<unknown>)}
	{#if feed.status === 'loading'}
		<p class="detail__note">Loading.</p>
	{:else if feed.status === 'missing'}
		<p class="detail__note">
			Not in the running mike yet. It appears here once this part of Wright is deployed.
		</p>
	{:else if feed.status === 'error'}
		<p class="detail__note detail__note--error" role="alert">
			Could not load: {feed.error}
			<Button variant="outline" size="sm" onclick={onRetry}>Retry</Button>
		</p>
	{:else if feed.rows.length === 0}
		<p class="detail__note">None.</p>
	{/if}
{/snippet}

{#snippet more(feed: Feed<unknown>, which: DetailFeed, what: string)}
	{#if feed.cursor !== null}
		<Button variant="outline" size="sm" disabled={feed.loadingMore} onclick={() => onMore(which)}>
			Load more {what}
		</Button>
	{/if}
{/snippet}

{#snippet section(title: string, body: Snippet)}
	<div class="detail__section">
		<h3 class="machine detail__h">{title}</h3>
		{@render body()}
	</div>
{/snippet}

{#snippet statementBody()}
	<p class="detail__statement" data-slot="wright-problem-statement">{problem.statement}</p>
{/snippet}

{#snippet evidenceBody()}
	<WrightFacts facts={problem.latest_evidence} label="Latest evidence" />
{/snippet}

{#snippet findingsBody()}
	{#if detail}
		{@render note(detail.findings)}
		{#each detail.findings.rows as f (f.id)}
			{@const chips = f.evidence.map((e): LinkChip => {
				const ref = findingEvidenceRef(e.kind, e.ref);
				return { icon: FileSearch, text: ref.text, href: ref.href, external: ref.external };
			})}
			<article class="detail__card" id="finding-{f.id}">
				<p class="detail__line">
					<strong>{f.symptom}</strong>
					<Badge variant={f.cause_established ? 'chip-active' : 'chip-muted'}>
						{f.cause_established ? 'Cause established' : 'Cause not established'}
					</Badge>
				</p>
				<p class="detail__text">Suspected: {f.suspected_cause}</p>
				{#if f.confirmed_cause}<p class="detail__text">Confirmed: {f.confirmed_cause}</p>{/if}
				<p class="detail__meta">
					{humanise(f.kind)}{#if f.owning_part}
						· owned by {f.owning_part}{/if} · {dayAndTime(f.created_at)}
				</p>
				<p class="detail__text">Remedy: {f.remedy}</p>
				<LinkChipRow items={chips} label="Evidence for finding {f.id}" />
			</article>
		{/each}
		{@render more(detail.findings, 'findings', 'findings')}
	{/if}
{/snippet}

{#snippet runsBody()}
	{#if detail}
		{@render note(detail.investigations)}
		{#each detail.investigations.rows as run (run.id)}
			<p class="detail__text" id="run-{run.id}">
				Run {run.id}: {humanise(run.state)}
				{#if run.started_at}· started {dayAndTime(run.started_at)}{/if}
				{#if run.ended_at}· finished {dayAndTime(run.ended_at)}{/if}
				{#if run.end_reason}· {run.end_reason}{/if}
				{#if run.session_uuid}
					<LinkChipRow
						items={[
							{
								icon: FileSearch,
								kind: 'Session',
								text: 'The run',
								href: `/sessions/${run.session_uuid}`
							}
						]}
					/>
				{/if}
			</p>
		{/each}
		{@render more(detail.investigations, 'investigations', 'runs')}
	{/if}
{/snippet}

{#snippet repairsBody()}
	{#if detail}
		{@render note(detail.repairs)}
		{#each detail.repairs.rows as r (r.id)}
			{@const chips = repairChips(r)}
			<article class="detail__card" id="repair-{r.id}">
				<ol class="steps" aria-label="Repair {r.id} progress">
					{#each repairSteps(r.state) as step (step.state)}
						<li
							class="steps__item"
							data-status={step.status}
							aria-current={step.status === 'current' ? 'step' : undefined}
						>
							{step.label}
						</li>
					{/each}
				</ol>
				{#if REPAIR_STOPPED[r.state]}
					<p class="detail__text" role="status">
						{REPAIR_STOPPED[r.state]}{r.reason ? ` ${r.reason}` : ''}
					</p>
				{/if}
				<p class="detail__meta">
					{humanise(r.state)}{#if r.branch}
						· branch {r.branch}{/if} · {dayAndTime(r.created_at)}
				</p>
				<LinkChipRow items={chips} label="Repair {r.id} links" />
			</article>
		{/each}
		{@render more(detail.repairs, 'repairs', 'repairs')}
	{/if}
{/snippet}

{#snippet actionsBody()}
	{#if detail}
		{@render note(detail.actions)}
		{#if detail.actions.rows.length > 0}
			<ul class="detail__list" role="list">
				{#each detail.actions.rows as action (action.id)}
					<WrightActionRow
						{action}
						{problem}
						showDay
						undo={undo[`action-${action.id}`]}
						{undoDisabledReason}
						{onUndo}
					/>
				{/each}
			</ul>
		{/if}
		{@render more(detail.actions, 'actions', 'actions')}
	{/if}
{/snippet}

{#snippet episodesBody()}
	{#if detail}
		{@render note(detail.episodes)}
		{#each detail.episodes.rows as ep (ep.id)}
			<article class="detail__card" id="episode-{ep.id}">
				<p class="detail__meta">
					Opened {dayAndTime(ep.opened_at)}
					{#if ep.closed_at}· closed {dayAndTime(ep.closed_at)}{:else}· still open{/if}
					· {ep.observations} observations
				</p>
				<WrightFacts facts={ep.evidence} label="Evidence of episode {ep.id}" />
			</article>
		{/each}
		{@render more(detail.episodes, 'episodes', 'episodes')}
	{/if}
{/snippet}

<div class="detail" data-slot="wright-problem-detail">
	{#if problem.statement !== null}
		{@render section('Statement', statementBody)}
	{/if}
	{@render section('Latest evidence', evidenceBody)}
	{@render section('What Wright found', findingsBody)}
	{@render section('Engineer runs', runsBody)}
	{@render section('Repairs', repairsBody)}
	{@render section('What it did', actionsBody)}
	{@render section('Episodes', episodesBody)}
</div>

<style>
	.detail {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.detail__section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.detail__h {
		margin: 0;
		font-size: var(--text-micro);
		color: var(--text-3);
	}

	.detail__card {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-2) 0;
		scroll-margin-top: var(--space-6);
	}

	.detail__card + .detail__card {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.detail__card:target {
		background: var(--surface-2);
	}

	.detail__list {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.detail__line,
	.detail__text {
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.detail__text {
		display: block;
	}

	.detail__statement {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.detail__meta,
	.detail__note {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}

	.detail__note--error {
		color: var(--status-danger-fg);
	}

	.steps {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
	}

	.steps__item {
		color: var(--text-3);
	}

	.steps__item[data-status='done'] {
		color: var(--text-1);
	}

	.steps__item[data-status='current'] {
		color: var(--text-strong);
	}
</style>
