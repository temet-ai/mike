<!--
@component
One thing the operator keeps saying: how many times, whether it is still happening, when, in which
projects, the twelve weekly counts as bars with every number written under them, and every message
in full, oldest first, each with a link to the session it came from. The issue button opens a
prefilled GitHub issue; when the whole list is too long for a link it carries the title only and
the full list is put on the clipboard, with a line below the button that says so. The line's space
is always reserved.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import { ArrowUpRight, ExternalLink } from '@lucide/svelte';
	import Badge from '$lib/components/shared/Badge.svelte';
	import { Button } from '$lib/components/ui/button';
	import EvalQuote from '$lib/components/eval/EvalQuote.svelte';
	import { formatWhen } from '$lib/components/eval/evalReview';
	import type { RepeatGroup } from '$lib/api/improvement';
	import {
		issueBody,
		issueHref,
		issueTooLong,
		spanText,
		weekCaption,
		weeklyReadout
	} from './repeatedAsks';

	interface Props {
		group: RepeatGroup;
		weekStarts: string[];
		scaleMax: number;
		now: number;
	}

	const { group, weekStarts, scaleMax, now }: Props = $props();

	let onClipboard = $state(false);

	const tooLong = $derived(issueTooLong(group));

	async function copyListWhenTooLong(): Promise<void> {
		if (!tooLong) return;
		try {
			await navigator.clipboard.writeText(issueBody(group));
			onClipboard = true;
		} catch {
			onClipboard = false;
		}
	}

	function barHeight(count: number): string {
		return `${(count / scaleMax) * 100}%`;
	}
</script>

<article class="ra-group" id="repeat-{group.id}" data-slot="repeated-ask-group">
	<header class="band-hd band-hd--static">
		<h3 class="machine ra-count">Said {group.n} times</h3>
		{#if group.live}
			<Badge variant="chip-warning">Still happening</Badge>
		{:else}
			<Badge variant="chip-muted">Quiet</Badge>
		{/if}
	</header>
	<div class="band-body ra-body">
		<div class="ra-facts">
			<dl class="ra-when">
				<dt>When</dt>
				<dd>{spanText(group, now)}</dd>
				<dt>Projects</dt>
				<dd>{group.projects.join(', ')}</dd>
			</dl>
			<div class="ra-weeks">
				<div class="ra-grid" aria-hidden="true">
					{#each group.weekly as count, i (weekStarts[i] ?? i)}
						<div class="ra-cell">
							<div class="ra-bar-box">
								<div
									class="ra-bar"
									class:ra-bar--zero={count === 0}
									style:height={count === 0 ? undefined : barHeight(count)}
								></div>
							</div>
							<span class="ra-num" class:ra-num--zero={count === 0} data-testid="ra-week-count"
								>{count}</span
							>
						</div>
					{/each}
				</div>
				<p class="ra-caption">{weekCaption(weekStarts)}</p>
				<p class="ra-sr">{weeklyReadout(group.weekly)}</p>
			</div>
		</div>
		<EvalQuote who="you" heading="What you said">
			<ol class="ra-items">
				{#each group.items as item (item.turn_id)}
					<li class="ra-item">
						<p class="ra-item-meta">
							{formatWhen(item.at)} · {item.project} ·
							<a class="ra-link" href={resolve(`sessions/${encodeURIComponent(item.session_id)}`)}
								>Open the session <ArrowUpRight size={12} aria-hidden="true" /></a
							>
						</p>
						<p class="ra-item-text">{item.text}</p>
					</li>
				{/each}
			</ol>
		</EvalQuote>
		<div class="ra-actions">
			<Button
				variant="outline"
				size="sm"
				href={issueHref(group)}
				target="_blank"
				rel="noopener noreferrer"
				onclick={copyListWhenTooLong}
			>
				File as a mike issue <ExternalLink size={12} aria-hidden="true" />
			</Button>
			<span class="ra-actions-note" class:ra-actions-note--shown={onClipboard} role="status">
				The list was too long for the link, so it is on your clipboard.
			</span>
		</div>
	</div>
</article>

<style>
	.ra-group {
		min-width: 0;
	}
	.ra-count {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.ra-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}
	.ra-facts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4);
		align-items: flex-start;
	}
	.ra-when {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		gap: var(--space-1) var(--space-3);
		margin: 0;
		min-width: 0;
		flex: 1 1 18rem;
	}
	.ra-when dt {
		color: var(--text-3);
		font-size: var(--text-sm);
	}
	.ra-when dd {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.ra-weeks {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.ra-grid {
		display: grid;
		grid-template-columns: repeat(12, 10px);
		gap: 3px;
	}
	.ra-cell {
		display: flex;
		flex-direction: column;
		align-items: center;
	}
	.ra-bar-box {
		display: flex;
		align-items: flex-end;
		width: 100%;
		height: 28px;
	}
	.ra-bar {
		width: 100%;
		background: var(--data-teal-fg);
	}
	.ra-bar--zero {
		height: var(--hairline);
		background: var(--border-subtle);
	}
	.ra-num {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
		color: var(--text-2);
	}
	.ra-num--zero {
		color: var(--text-3);
	}
	.ra-caption {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.ra-sr {
		position: absolute;
		width: 1px;
		height: 1px;
		margin: -1px;
		padding: 0;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
		border: 0;
	}
	.ra-items {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.ra-item {
		padding-top: var(--space-3);
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.ra-item + .ra-item {
		margin-top: var(--space-3);
	}
	.ra-item:first-child {
		padding-top: 0;
		border-top: 0;
	}
	.ra-item-meta {
		margin: 0 0 var(--space-1);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.ra-item-text {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.ra-link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.ra-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ra-actions {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-1);
	}
	.ra-actions-note {
		font-size: var(--text-xs);
		color: var(--text-2);
		visibility: hidden;
	}
	.ra-actions-note--shown {
		visibility: visible;
	}
</style>
