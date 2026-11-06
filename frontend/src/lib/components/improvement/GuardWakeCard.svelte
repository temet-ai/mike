<!--
@component
One delivered guard nudge as a compact band: the question in plain words (click it to expand),
"Was this right?" with Right and Wrong beside it once the next turn has been judged, and a source
line with what the guard did, where, when, and how the next turn went. Expanding reveals the note
the guard added in full, the full outcome sentence and a link to the turn. Used on
`/sessions/improvement` (Nudges tab) and on `/sessions/improvement/guard`.
-->
<script lang="ts">
	import { ArrowUpRight } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import EvalQuote from '$lib/components/eval/EvalQuote.svelte';
	import { formatClock, formatDay } from '$lib/components/eval/evalReview';
	import { disclose } from '$lib/components/ui/motion';
	import { isJudged } from '$lib/api/guardEffect';
	import type { GuardWake } from '$lib/api/guardEffect';

	interface Props {
		wake: GuardWake;
		busy: boolean;
		showSession?: boolean;
		onLabel: (label: 'right' | 'wrong') => Promise<void>;
	}

	let { wake, busy, showSession = true, onLabel }: Props = $props();

	type Outcome = NonNullable<GuardWake['outcome']>;

	const OUTCOME_TEXT: Partial<Record<Outcome, string>> = {
		accepted: 'The next turn accepted it and moved on.',
		redirected:
			'The next turn changed direction — not because anything was wrong, just a change of plan.',
		corrected: 'The next turn pointed out something the assistant got wrong.',
		closed_in_frustration: 'The session ended in frustration right after.',
		no_response: 'Nobody replied after the next turn, so there is no reaction to go on.',
		unclear: 'The judge read the next turn but could not tell how it landed.'
	};

	const OUTCOME_TAG: Record<Outcome, { variant: BadgeVariant; text: string }> = {
		accepted: { variant: 'tag-green', text: 'Next turn went on' },
		corrected: { variant: 'tag-amber', text: 'Next turn corrected it' },
		closed_in_frustration: { variant: 'tag-amber', text: 'Session ended upset' },
		redirected: { variant: 'tag-default', text: 'Next turn changed plan' },
		no_response: { variant: 'tag-default', text: 'No reply after' },
		unclear: { variant: 'tag-default', text: 'Unclear' },
		unknown: { variant: 'tag-default', text: 'Unclear' }
	};

	let open = $state(false);
	let clicked = $state<'right' | 'wrong' | null>(null);
	let rowError = $state<string | null>(null);

	const judged = $derived(isJudged(wake));
	const bodyId = $derived(`gwb-body-${wake.evaluationUuid}`);
	const tag = $derived(judged && wake.outcome !== null ? OUTCOME_TAG[wake.outcome] : null);
	const outcomeText = $derived(
		judged && wake.outcome !== null
			? (OUTCOME_TEXT[wake.outcome] ?? '')
			: 'The next turn has not been judged yet. Once it has, this will ask whether the guard was right.'
	);
	const when = $derived(
		[formatDay(wake.deliveredAt), formatClock(wake.deliveredAt)].filter(Boolean).join(' ')
	);
	const repeats = $derived(wake.repeatUuids?.length ?? 0);

	async function label(value: 'right' | 'wrong'): Promise<void> {
		clicked = value;
		rowError = null;
		try {
			await onLabel(value);
		} catch (e) {
			rowError = e instanceof Error ? e.message : String(e);
		}
	}
</script>

<article class="gwb" data-slot="guard-wake-card" aria-label="Delivered guard nudge">
	<div class="band-hd-wrap gwb-hd">
		<button
			type="button"
			class="band-hd band-hd--top"
			aria-expanded={open}
			aria-controls={bodyId}
			onclick={() => (open = !open)}
		>
			<DisclosureMarker {open} size={10} />
			<span class="gwb-q">{wake.questionLabel}</span>
		</button>
		<div class="gwb-act">
			{#if wake.operatorLabel}
				<span class="gwb-said">You said {wake.operatorLabel === 'right' ? 'right' : 'wrong'}</span>
			{:else if judged}
				<span class="gwb-ask">Was this right?</span>
				<Button
					variant="outline"
					size="sm"
					disabled={busy}
					aria-busy={busy && clicked === 'right'}
					onclick={() => void label('right')}>Right</Button
				>
				<Button
					variant="outline"
					size="sm"
					disabled={busy}
					aria-busy={busy && clicked === 'wrong'}
					onclick={() => void label('wrong')}>Wrong</Button
				>
			{:else}
				<span class="gwb-later">Ask once judged</span>
			{/if}
		</div>
	</div>
	<p class="gwb-src">
		<span>{wake.arm === 'wake' ? 'Woke the model' : 'Added a note to the reply'}</span>
		{#if showSession}<span
				>· in <a class="gwb-link" href={wake.sessionHref}>{wake.sessionTitle}</a></span
			>{/if}
		{#if when}<span>· {when}</span>{/if}
		{#if repeats > 0}<span>· {repeats + 1} times in one turn, one answer covers all</span>{/if}
		{#if tag}<span class="gwb-tag"><Badge variant={tag.variant}>{tag.text}</Badge></span>{/if}
	</p>
	{#if open}
		<div class="gwb-body" id={bodyId} transition:disclose>
			<div class="band-body gwb-inner">
				<EvalQuote
					who="guard"
					heading={wake.arm === 'wake' ? 'What it told the model' : 'The note it added'}
				>
					{#if wake.noteText}
						<p class="gwb-note">{wake.noteText}</p>
					{:else}
						<p class="gwb-note gwb-missing">The text of this note was not recorded.</p>
					{/if}
				</EvalQuote>
				<p class="gwb-outcome">{outcomeText}</p>
				<div class="gwb-links">
					<a class="gwb-link" href={wake.turnHref}
						>Open the turn<ArrowUpRight size={12} aria-hidden="true" /></a
					>
				</div>
			</div>
		</div>
	{/if}
	{#if rowError}
		<p class="gwb-error" role="alert">{rowError}</p>
	{/if}
</article>

<style>
	.gwb {
		min-width: 0;
	}
	.gwb-q {
		font-size: var(--text-md);
		font-weight: var(--weight-medium);
		color: var(--text-1);
		line-height: var(--leading-body);
		overflow-wrap: anywhere;
		text-align: left;
	}
	.gwb-act {
		display: flex;
		flex: 0 0 auto;
		gap: var(--space-2);
		align-items: center;
		justify-content: flex-end;
		min-width: 15rem;
		padding-right: var(--space-4);
		/* the height of a size="sm" Button (h-7), held when a word replaces the buttons */
		min-height: 1.75rem;
	}
	.gwb-ask {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.gwb-said {
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.gwb-later {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.gwb-src {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0 var(--space-2);
		margin: 0;
		padding: 0 var(--space-4) var(--space-3) calc(var(--space-4) + 10px + var(--space-2));
		font-size: var(--text-sm);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.gwb-tag {
		margin-left: auto;
	}
	.gwb-body {
		display: grid;
	}
	.gwb-inner {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-height: 0;
		overflow: hidden;
		padding-left: calc(var(--space-4) + 10px + var(--space-2));
	}
	.gwb-note {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.gwb-missing {
		color: var(--text-2);
	}
	.gwb-outcome {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.gwb-links {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4);
		font-size: var(--text-sm);
	}
	.gwb-link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--interactive-active-fg);
		text-underline-offset: var(--space-1);
	}
	.gwb-src .gwb-link {
		display: inline;
	}
	.gwb-link:hover {
		text-decoration: underline;
	}
	.gwb-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.gwb-error {
		margin: 0;
		padding: 0 var(--space-4) var(--space-3) calc(var(--space-4) + 10px + var(--space-2));
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
	}
	@container (max-width: 719.98px) {
		.gwb-hd {
			flex-wrap: wrap;
		}
		.gwb-act {
			flex-basis: 100%;
			justify-content: flex-start;
			min-width: 0;
			padding: 0 var(--space-4) var(--space-2) calc(var(--space-4) + 10px + var(--space-2));
		}
		.gwb-tag {
			margin-left: 0;
		}
	}
</style>
