<!--
@component
The turn being decided, as one band of the spine's card, in the order it happened: what you wrote,
what the assistant did, its last message, and your reply. Each piece of conversation is an
attributed quote. `keyAxes` puts the blue key-evidence edge on the block each asked question turns
on: the last message for "What was delivered", your reply for "How you reacted". The band ends with
the one link into the session, which lands on the session page scrolled to this turn.
-->
<script lang="ts">
	import { ArrowUpRight } from '@lucide/svelte';
	import type { EvalReviewContextDto } from '$lib/api/evalRuns';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import MarkdownText from '$lib/components/shared/MarkdownText.svelte';
	import EvalQuote from './EvalQuote.svelte';
	import {
		formatClock,
		formatDuration,
		formatWhen,
		markSegments,
		stillRunning,
		stopWords
	} from './evalReview';
	import type { Axis, CitedSpan, FactsState } from './evalReview';

	interface Props {
		context: EvalReviewContextDto | null;
		facts: FactsState;
		keyAxes: Axis[];
		cited: CitedSpan | null;
		turnNumber: number | null;
		nextTurnNumber: number | null;
		turnHref: string | null;
		onRetryFacts: () => void;
	}

	let {
		context,
		facts,
		keyAxes,
		cited,
		turnNumber,
		nextTurnNumber,
		turnHref,
		onRetryFacts
	}: Props = $props();

	let filesOpen = $state(false);

	const known = $derived(facts.status === 'ok' ? facts.facts : null);
	const askedAt = $derived(formatClock(known?.created_at));
	const repliedAt = $derived(formatClock(context?.next_genuine_at));
	const endedAt = $derived.by(() => {
		const start = known?.created_at ? Date.parse(known.created_at) : NaN;
		if (Number.isNaN(start) || known?.duration_ms == null) return null;
		return formatClock(new Date(start + known.duration_ms).toISOString());
	});
	const running = $derived(stillRunning(known, context));
	const stopped = $derived(stopWords(known));
	const duration = $derived(formatDuration(known?.duration_ms));
	const lastMessage = $derived(known?.stop_response ? known.stop_response : null);
	const keyDelivery = $derived(keyAxes.includes('delivery'));
	const keyReaction = $derived(keyAxes.includes('reaction'));
	const replyMeta = $derived(
		[
			nextTurnNumber !== null ? `this became turn ${nextTurnNumber}` : null,
			cited?.key === 'reply' ? 'marked: the part the labelling quoted' : null
		]
			.filter(Boolean)
			.join(' · ')
	);
	const title = $derived(turnNumber !== null ? `This turn · turn ${turnNumber}` : 'This turn');

	function at(word: string, time: string | null): string {
		return time ? `${word}, ${time}` : word;
	}

	function nextMissing(c: EvalReviewContextDto): string {
		if (c.next_message_state === 'none') return 'You sent no further message in this session.';
		if (c.next_message_state === 'pending') return 'No next message yet.';
		if (c.next_message_state === 'missing')
			return 'Your next message exists but its text is not stored.';
		return 'No record of your next message.';
	}
</script>

{#snippet marked(text: string, key: string)}{@const seg =
		cited?.key === key ? markSegments(text, cited.span) : null}{#if seg}{seg.before}<mark
			class="ete-mark">{seg.match}</mark
		>{seg.after}{:else}{text}{/if}{/snippet}

{#snippet markedTag(key: string)}
	{#if cited?.key === key}marked: the part the labelling quoted{/if}
{/snippet}

<section class="ete" aria-label="This turn" data-testid="turn-evidence">
	<div class="band-hd band-hd--static">
		<span class="machine ete-label">{title}</span>
		{#if known?.created_at}<span class="band-hd__meta">{formatWhen(known.created_at)}</span>{/if}
	</div>
	<div class="band-body ete-body">
		{#if !context}
			<p class="ete-missing">
				The stored record of this turn is gone, so there is nothing to show.
			</p>
		{:else}
			{#if context.prompt_state === 'present' && context.user_prompt}
				<EvalQuote who="you" heading={at('You wrote', askedAt)} testid="operator-prompt">
					{#snippet meta()}{@render markedTag('prompt')}{/snippet}
					<p class="ete-pre">{@render marked(context.user_prompt, 'prompt')}</p>
				</EvalQuote>
			{:else}
				<p class="ete-missing" data-testid="operator-prompt">
					Your prompt was not captured for this turn.
				</p>
			{/if}

			{#if facts.status === 'error'}
				<p class="ete-error" role="alert" data-testid="facts-error">
					What the assistant did could not be loaded ({facts.message}).
					<button type="button" class="ete-retry" onclick={onRetryFacts}>Try again</button>
				</p>
			{:else}
				<div class="ete-facts" data-testid="turn-facts">
					<span class="machine ete-k">What it did</span>
					<div class="ete-fact-list">
						{#if !known}
							<span class="ete-quiet">Loading</span>
						{:else}
							{#if duration}<span>ran {duration}</span>{/if}
							{#if known.tool_call_count !== null}
								<span
									>{known.tool_call_count} tool calls{#if known.tool_error_count}, {known.tool_error_count}
										returned an error{/if}</span
								>
							{/if}
							{#if known.files_touched.length > 0}
								<button
									type="button"
									class="ete-toggle"
									aria-expanded={filesOpen}
									onclick={() => (filesOpen = !filesOpen)}
								>
									<DisclosureMarker open={filesOpen} size={10} /><span
										>{known.files_touched.length} files read or edited</span
									>
								</button>
							{/if}
							{#if stopped}<span>{stopped}</span>{/if}
							{#if running !== null}
								<span class="ete-hot" data-testid="running-words"
									>{running
										? 'still running when you replied'
										: 'finished before you replied'}</span
								>
							{/if}
						{/if}
					</div>
					{#if filesOpen && known}
						<!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scroll container must take focus so the keyboard can scroll it -->
						<div
							class="ete-files-list"
							tabindex="0"
							role="region"
							aria-label="Files read or edited"
						>
							<ul>
								{#each known.files_touched as file, i (i)}<li>{file}</li>{/each}
							</ul>
						</div>
					{/if}
				</div>
			{/if}

			{#if context.assistant_state === 'empty_turn'}
				<p class="ete-missing" data-testid="assistant-text">
					The assistant wrote no text in this turn.
				</p>
			{:else if lastMessage}
				<EvalQuote
					who="claude"
					heading={at("Claude's last message", endedAt)}
					key={keyDelivery}
					testid="assistant-text"
				>
					{#snippet meta()}{@render markedTag('last')}{/snippet}
					{#if cited?.key === 'last'}
						<p class="ete-pre">{@render marked(lastMessage, 'last')}</p>
					{:else}
						<MarkdownText text={lastMessage} />
					{/if}
				</EvalQuote>
			{:else if facts.status === 'ok'}
				<p class="ete-missing" data-testid="assistant-text">
					No last message from the assistant is stored for this turn.
				</p>
			{/if}

			{#if context.next_message_state === 'present' && context.next_genuine_text}
				<EvalQuote
					who="you"
					heading={at('You replied', repliedAt)}
					key={keyReaction}
					testid="next-message"
				>
					{#snippet meta()}{replyMeta}{/snippet}
					<p class="ete-pre">{@render marked(context.next_genuine_text, 'reply')}</p>
				</EvalQuote>
			{:else}
				<p class="ete-missing" data-testid="next-message">{nextMissing(context)}</p>
			{/if}
		{/if}

		{#if turnHref}
			<a class="ete-link" href={turnHref} data-testid="turn-link"
				>See every step of this turn in the session<ArrowUpRight size={12} aria-hidden="true" /></a
			>
		{/if}
	</div>
</section>

<style>
	.ete {
		min-width: 0;
	}
	.ete-label {
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.ete-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}
	.ete-k {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.ete-pre {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.ete-mark {
		background: var(--status-info-bg);
		border-bottom: var(--hairline) solid var(--status-info-border);
		color: inherit;
	}
	.ete-missing {
		margin: 0;
		padding: var(--space-3);
		border: var(--hairline) dashed var(--border-strong);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.ete-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	.ete-retry {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--interactive-active-fg);
		cursor: pointer;
	}
	.ete-retry:hover {
		text-decoration: underline;
	}
	.ete-retry:focus-visible,
	.ete-toggle:focus-visible,
	.ete-files-list:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ete-facts {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-2) 0;
		border-top: var(--hairline) solid var(--border-subtle);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.ete-fact-list {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-4);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.ete-hot {
		font-weight: var(--weight-display);
	}
	.ete-quiet {
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.ete-files-list {
		padding: var(--space-2) var(--space-3);
		max-block-size: 60vh;
		overflow-y: auto;
		background: var(--surface-2);
		border: var(--hairline) solid var(--border-subtle);
		font-size: var(--text-xs);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.ete-files-list ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.ete-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-sm);
		color: var(--text-1);
		text-align: left;
		cursor: pointer;
	}
	.ete-toggle:hover {
		color: var(--interactive-active-fg);
	}
	.ete-link {
		display: inline-flex;
		align-items: center;
		align-self: flex-start;
		gap: var(--space-1);
		font-size: var(--text-sm);
		color: var(--interactive-active-fg);
	}
	.ete-link:hover {
		text-decoration: underline;
	}
</style>
