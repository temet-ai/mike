<!--
@component
The session around the turn being decided, as two bands of the spine's card: "The session" (what it
was for and how it went, from the session summary, with the earlier turns one click away) and "Just
before", the previous turn in full. Both render as direct children so the parent `.mech-group` draws
one frame with hairline seams.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import { ArrowUpRight } from '@lucide/svelte';
	import type { OperatorTurnDto, SessionSummary } from '$lib/types';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import MarkdownText from '$lib/components/shared/MarkdownText.svelte';
	import EvalQuote from './EvalQuote.svelte';
	import { formatClock, formatDate, formatWhen } from './evalReview';
	import type { TurnPosition } from './evalReview';

	interface Props {
		sessionUuid: string;
		summary: SessionSummary | null;
		summaryError: string | null;
		turns: OperatorTurnDto[] | null;
		turnsError: string | null;
		position: TurnPosition | null;
		open: boolean;
		onToggle: (open: boolean) => void;
		onRetry: () => void;
	}

	let {
		sessionUuid,
		summary,
		summaryError,
		turns,
		turnsError,
		position,
		open,
		onToggle,
		onRetry
	}: Props = $props();

	let earlierOpen = $state(false);
	let lastOpen = $state<Record<number, boolean>>({});

	const sessionHref = $derived(
		resolve(`/sessions/${encodeURIComponent(sessionUuid)}` as `/sessions/${string}`)
	);
	const previous = $derived(
		position && turns && position.index >= 2 ? (turns[position.index - 2] ?? null) : null
	);
	const earlier = $derived(
		position && turns && position.index >= 3 ? turns.filter((_, i) => i < position.index - 2) : []
	);
	const paragraphs = $derived(
		(summary?.narrative_overview ?? '')
			.split(/\n\s*\n/)
			.map((p) => p.trim())
			.filter((p) => p !== '')
	);
	const span = $derived.by(() => {
		if (!summary) return null;
		const from = formatWhen(summary.first_seen);
		const sameDay = formatDate(summary.first_seen) === formatDate(summary.last_seen);
		const to = sameDay ? formatClock(summary.last_seen) : formatWhen(summary.last_seen);
		return from && to ? `${from} to ${to}` : null;
	});
	const meta = $derived(
		[turns && position ? `${turns.length} turns` : null, span].filter(Boolean).join(' · ')
	);

	function endOf(turn: OperatorTurnDto): string | null {
		const start = Date.parse(turn.created_at);
		if (Number.isNaN(start)) return null;
		return formatClock(new Date(start + (turn.duration_ms ?? 0)).toISOString());
	}
</script>

{#snippet lastMessage(turn: OperatorTurnDto, n: number, scroll: boolean)}
	{#if turn.stop_response}
		<EvalQuote
			who="claude"
			heading={`Claude's last message${endOf(turn) ? `, ${endOf(turn)}` : ''}`}
			{scroll}
			testid={scroll ? 'previous-last-message' : undefined}
		>
			<MarkdownText text={turn.stop_response} />
		</EvalQuote>
	{:else}
		<p class="esc-missing">The assistant wrote no text in turn {n}.</p>
	{/if}
{/snippet}

<section class="esc-band" aria-label="The session" data-testid="session-band">
	<div class="band-hd-wrap">
		<button
			type="button"
			class="band-hd"
			aria-expanded={open}
			onclick={() => onToggle(!open)}
		>
			<DisclosureMarker {open} size={10} />
			<span class="machine esc-label">The session</span>
			{#if meta}<span class="band-hd__meta">{meta}</span>{/if}
		</button>
	</div>
	{#if open}
		<div class="band-body esc-body">
			{#if summaryError}
				<p class="esc-error" role="alert">
					The session could not be loaded ({summaryError}).
					<button type="button" class="esc-link-btn" onclick={onRetry}>Try again</button>
				</p>
			{:else if !summary}
				<p class="esc-quiet">Loading the session</p>
			{:else}
				<div class="esc-title-row">
					<h3 class="esc-title">{summary.title ?? 'Untitled session'}</h3>
					<a class="esc-link" href={sessionHref} data-testid="open-session"
						>Open the session<ArrowUpRight size={12} aria-hidden="true" /></a
					>
				</div>
				{#if summary.project ?? summary.cwd}<p class="esc-path">
						{summary.project ?? summary.cwd}
					</p>{/if}
				{#if paragraphs.length > 0}
					<div class="esc-overview" data-testid="session-overview">
						{#each paragraphs as p, i (i)}<p>{p}</p>{/each}
					</div>
				{:else}
					<p class="esc-missing">No summary of this session has been written yet.</p>
				{/if}
			{/if}
			{#if earlier.length > 0 && position}
				<button
					type="button"
					class="esc-toggle"
					aria-expanded={earlierOpen}
					onclick={() => (earlierOpen = !earlierOpen)}
				>
					<DisclosureMarker open={earlierOpen} size={10} /><span
						>Turns 1 to {position.index - 2}</span
					>
				</button>
				{#if earlierOpen}
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scroll container must take focus so the keyboard can scroll it -->
					<ol
						class="esc-earlier"
						tabindex="0"
						aria-label={`Turns 1 to ${position.index - 2}`}
						data-testid="earlier-turns"
					>
						{#each earlier as turn, i (turn.turn_id)}
							<li class="esc-turn">
								<span class="machine esc-label">Turn {i + 1}</span>
								<EvalQuote
									who="you"
									heading={`You wrote${formatClock(turn.created_at) ? `, ${formatClock(turn.created_at)}` : ''}`}
								>
									<p class="esc-pre">{turn.user_prompt}</p>
								</EvalQuote>
								<button
									type="button"
									class="esc-toggle"
									aria-expanded={lastOpen[turn.turn_id] ?? false}
									onclick={() =>
										(lastOpen = { ...lastOpen, [turn.turn_id]: !(lastOpen[turn.turn_id] ?? false) })}
								>
									<DisclosureMarker open={lastOpen[turn.turn_id] ?? false} size={10} /><span
										>Its last message</span
									>
								</button>
								{#if lastOpen[turn.turn_id]}{@render lastMessage(turn, i + 1, false)}{/if}
							</li>
						{/each}
					</ol>
				{/if}
			{/if}
		</div>
	{/if}
</section>

{#if position && position.index >= 2}
	<section class="esc-band" aria-label="Just before" data-testid="just-before">
		<div class="band-hd band-hd--static">
			<span class="machine esc-label">Just before · turn {position.index - 1}</span>
			{#if previous}<span class="band-hd__meta">{formatWhen(previous.created_at)}</span>{/if}
		</div>
		<div class="band-body esc-body">
			{#if previous}
				<EvalQuote
					who="you"
					heading={`You wrote${formatClock(previous.created_at) ? `, ${formatClock(previous.created_at)}` : ''}`}
					testid="previous-prompt"
				>
					<p class="esc-pre">{previous.user_prompt}</p>
				</EvalQuote>
				{@render lastMessage(previous, position.index - 1, true)}
			{/if}
		</div>
	</section>
{:else if turnsError}
	<section class="esc-band" aria-label="Just before">
		<div class="band-body esc-body esc-first">
			<p class="esc-error" role="alert">
				The earlier turns could not be loaded ({turnsError}).
				<button type="button" class="esc-link-btn" onclick={onRetry}>Try again</button>
			</p>
		</div>
	</section>
{/if}

<style>
	.esc-band {
		min-width: 0;
	}
	.esc-label {
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.esc-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}
	.esc-first {
		padding-top: var(--space-3);
	}
	.esc-title-row {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-3);
	}
	.esc-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-display);
		color: var(--text-1);
	}
	.esc-link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		font-size: var(--text-xs);
		color: var(--interactive-active-fg);
	}
	.esc-link:hover {
		text-decoration: underline;
	}
	.esc-path {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.esc-overview {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.esc-overview p {
		margin: 0;
	}
	.esc-pre {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.esc-missing {
		margin: 0;
		padding: var(--space-3);
		border: var(--hairline) dashed var(--border-strong);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.esc-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	.esc-quiet {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.esc-link-btn {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--interactive-active-fg);
		cursor: pointer;
	}
	.esc-link-btn:hover {
		text-decoration: underline;
	}
	.esc-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-xs);
		color: var(--text-2);
		cursor: pointer;
	}
	.esc-toggle:hover {
		color: var(--text-1);
	}
	.esc-toggle:focus-visible,
	.esc-link-btn:focus-visible,
	.esc-earlier:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.esc-earlier {
		list-style: none;
		margin: 0;
		padding: 0 var(--space-2) 0 0;
		max-block-size: 60vh;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.esc-turn {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-top: var(--space-3);
		border-top: var(--hairline) solid var(--border-subtle);
	}
</style>
