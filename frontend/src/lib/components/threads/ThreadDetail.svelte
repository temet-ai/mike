<!--
@component
One thread in one card, sections divided by hairlines: identity and goal, current state, acceptance
criteria, dependencies, linked sessions and history. The state is what the last checkpoint wrote.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import Badge from '$lib/components/shared/Badge.svelte';
	import { formatDay } from '$lib/components/eval/evalReview';
	import type { ThreadDetail } from '$lib/api/workThreads';
	import ThreadHistory from './ThreadHistory.svelte';
	import { criteriaLines, splitLines, statusLabel, statusVariant } from './threadFormat';

	let {
		detail,
		hrefFor,
		actions
	}: {
		detail: ThreadDetail;
		hrefFor: (slug: string) => string;
		actions?: Snippet;
	} = $props();

	const thread = $derived(detail.thread);
	const state = $derived(detail.state);
	const criteria = $derived(criteriaLines(detail.acceptance_criteria));
	const stateParts = $derived(
		state
			? [
					{ label: 'Where we are', text: state.where_we_are },
					{ label: 'Next up', text: state.next_up },
					{ label: 'Blockers', text: state.blockers },
					{ label: 'Open questions', text: state.open_questions },
					{ label: 'Decisions', text: state.decisions }
				].filter((part) => part.text.trim().length > 0)
			: []
	);
</script>

<article class="td" data-slot="thread-detail" aria-label={thread.title ?? thread.slug}>
	<header class="td-sec td-head">
		<div class="td-titles">
			<h2 class="td-title">{thread.title ?? thread.slug}</h2>
			<p class="td-meta">
				{thread.slug} · started {formatDay(thread.created_at) ?? thread.created_at} · changed {formatDay(
					thread.updated_at
				) ?? thread.updated_at}
			</p>
		</div>
		<Badge variant={statusVariant(thread.status)}>{statusLabel(thread.status)}</Badge>
		{#if actions}<div class="td-actions">{@render actions()}</div>{/if}
		{#if thread.goal}<p class="td-goal">{thread.goal}</p>{/if}
		<dl class="td-facts">
			<div>
				<dt>Branch</dt>
				<dd>{thread.branch ?? '—'}</dd>
			</div>
			<div>
				<dt>Worktree</dt>
				<dd>{thread.worktree ?? '—'}</dd>
			</div>
			{#if thread.source_path}
				<div>
					<dt>Imported from</dt>
					<dd>{thread.source_path}</dd>
				</div>
			{/if}
		</dl>
		{#if thread.parse_error}
			<p class="td-error">Import problem: {thread.parse_error}</p>
		{/if}
	</header>

	<section class="td-sec" aria-label="State">
		<h3 class="td-h">State</h3>
		{#if state}
			<p class="td-tldr">{state.tldr}</p>
			{#each stateParts as part (part.label)}
				<div class="td-part">
					<h4 class="td-label">{part.label}</h4>
					<p class="td-text">{part.text}</p>
				</div>
			{/each}
			{#if state.files.length > 0}
				<div class="td-part">
					<h4 class="td-label">Files the next session reads</h4>
					<ul class="td-files">
						{#each state.files as file (file)}
							<li><code>{file}</code></li>
						{/each}
					</ul>
				</div>
			{/if}
			<p class="td-meta">Last checkpoint {formatDay(state.updated_at) ?? state.updated_at}</p>
		{:else}
			<p class="td-meta">No checkpoint yet.</p>
		{/if}
	</section>

	<section class="td-sec" aria-label="Acceptance criteria">
		<h3 class="td-h">Acceptance criteria</h3>
		{#if criteria.length > 0}
			<ul class="td-list">
				{#each criteria as line, i (i)}
					<li>{line}</li>
				{/each}
			</ul>
		{:else}
			<p class="td-meta">None set. A thread needs them before it can move to review.</p>
		{/if}
	</section>

	<section class="td-sec td-two" aria-label="Dependencies">
		<div>
			<h3 class="td-h">Depends on</h3>
			{#if detail.depends_on.length > 0}
				<ul class="td-list">
					{#each detail.depends_on as slug (slug)}
						<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- hrefFor builds the route with resolve() in the page -->
						<li><a href={hrefFor(slug)}>{slug}</a></li>
					{/each}
				</ul>
			{:else}
				<p class="td-meta">Nothing.</p>
			{/if}
		</div>
		<div>
			<h3 class="td-h">Needed by</h3>
			{#if detail.depended_on_by.length > 0}
				<ul class="td-list">
					{#each detail.depended_on_by as slug (slug)}
						<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- hrefFor builds the route with resolve() in the page -->
						<li><a href={hrefFor(slug)}>{slug}</a></li>
					{/each}
				</ul>
			{:else}
				<p class="td-meta">Nothing.</p>
			{/if}
		</div>
	</section>

	<section class="td-sec" aria-label="Sessions">
		<h3 class="td-h">Sessions</h3>
		{#if detail.sessions.length > 0}
			<ul class="td-sessions">
				{#each detail.sessions as link (link.session_uuid)}
					<li>
						<a href={resolve('/sessions/[sessionId]', { sessionId: link.session_uuid })}
							>{link.session_uuid}</a
						>
						<span class="td-meta">
							{formatDay(link.first_seen_at) ?? link.first_seen_at}
							to {formatDay(link.last_seen_at) ?? link.last_seen_at}
						</span>
						{#if splitLines(link.contribution).length > 0}
							<span class="td-text">{link.contribution}</span>
						{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p class="td-meta">No session has worked on this thread.</p>
		{/if}
	</section>

	<div class="td-sec">
		<ThreadHistory entries={detail.history} total={detail.history_total} />
	</div>
</article>

<style>
	.td {
		display: flex;
		flex-direction: column;
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		min-width: 0;
	}
	.td-sec {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-4);
		border-top: 1px solid var(--border-subtle);
	}
	.td-sec:first-child {
		border-top: 0;
	}
	.td-head {
		flex-direction: row;
		flex-wrap: wrap;
		align-items: flex-start;
	}
	.td-titles {
		flex: 1 1 16rem;
		min-width: 0;
	}
	.td-title {
		margin: 0;
		font-size: var(--text-lg);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.td-actions {
		display: flex;
		gap: var(--space-2);
	}
	.td-goal,
	.td-facts,
	.td-error {
		flex-basis: 100%;
		margin: 0;
	}
	.td-goal {
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.td-facts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-5);
		font-size: var(--text-xs);
	}
	.td-facts dt {
		color: var(--text-3);
	}
	.td-facts dd {
		margin: 0;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.td-error {
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
	}
	.td-h {
		margin: 0;
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
	}
	.td-tldr {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.td-part {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.td-label {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: 600;
		color: var(--text-3);
	}
	.td-text {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.td-meta {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
	.td-files,
	.td-list,
	.td-sessions {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.td-sessions li {
		display: flex;
		flex-direction: column;
	}
	.td-two {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
		gap: var(--space-4);
	}
</style>
