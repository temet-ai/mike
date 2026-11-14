<!--
@component
The pinned decision panel of the Review labels page. It asks "Which is right?" and every
answer of each disputed question is a visible button: the two disputed answers first, named by who
gave them, then "Something else:" and the rest in vocabulary order. One question saves on click; two
questions save once both rows have an answer. The status sits in the header row, so the panel's
height never changes with it.
-->
<script lang="ts" module>
	import type { VocabularyLabel } from '$lib/api/eval';
	import type { Axis, Candidate } from './evalReview';

	export interface BarQuestion {
		axis: Axis;
		candidates: Candidate[];
		others: VocabularyLabel[];
		saved: string | null;
	}
</script>

<script lang="ts">
	import { MessageSquarePlus } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Popover from '$lib/components/ui/popover';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { ROW_LABEL, formatWhen, joinWords } from './evalReview';
	import type { BarStatus } from './evalReview';
	import { wordFor } from './labelControl';

	interface Props {
		questions: BarQuestion[];
		suggestion?: { label: string; probability: number } | null;
		savedAt: string | null;
		busy: boolean;
		status: BarStatus | null;
		note: string;
		height?: number;
		onDecide: (answers: Array<{ axis: Axis; label: string }>) => void;
		onStatus: (status: BarStatus | null) => void;
		onGoBack: (turn: number) => void;
	}

	let {
		questions,
		suggestion,
		savedAt,
		busy,
		status,
		note = $bindable(''),
		height = $bindable(0),
		onDecide,
		onStatus,
		onGoBack
	}: Props = $props();

	let picks = $state<Partial<Record<Axis, string>>>({});
	let noteOpen = $state(false);

	const single = $derived(questions.length === 1);
	const suggests = $derived(
		suggestion !== undefined && questions.some((q) => q.axis === 'reaction')
	);
	const suggestionText = $derived(
		suggestion
			? `Jev suggests ${wordFor(suggestion.label)}, ${Math.round(suggestion.probability * 100)}%`
			: '—'
	);
	const pending = $derived(note.trim() !== '');

	function chosen(q: BarQuestion): string | null {
		return picks[q.axis] ?? q.saved;
	}

	function words(labels: Array<string | null>): string {
		return joinWords(labels.filter((l): l is string => l !== null).map(wordFor));
	}

	function order(q: BarQuestion): string[] {
		return [...q.candidates.map((c) => c.label), ...q.others.map((o) => o.label)];
	}

	const partial = $derived(
		!single && questions.some((q) => picks[q.axis]) && questions.some((q) => !chosen(q))
	);
	const decidedText = $derived.by(() => {
		const saved = questions.map((q) => q.saved);
		if (!savedAt || saved.every((s) => s === null)) return null;
		const when = formatWhen(savedAt);
		return `You chose ${words(saved)}${when ? ` on ${when}` : ''}. Pick another to change it.`;
	});
	const line = $derived<BarStatus | null>(
		status ??
			(partial
				? {
						tone: 'muted',
						text: 'Now answer the other row; the turn saves once both have an answer.'
					}
				: decidedText
					? { tone: 'muted', text: decidedText }
					: single
						? null
						: {
								tone: 'muted',
								text: 'Two questions: pick an answer in each row. The turn saves once both have one.'
							})
	);

	function choose(q: BarQuestion, label: string) {
		if (busy) return;
		if (single) {
			if (label === q.saved)
				onStatus({ tone: 'muted', text: `Already saved as ${wordFor(label)}.` });
			else onDecide([{ axis: q.axis, label }]);
			return;
		}
		picks = { ...picks, [q.axis]: label };
		if (questions.some((x) => !chosen(x))) {
			onStatus(null);
			return;
		}
		const answers = questions.flatMap((x) => {
			const picked = picks[x.axis];
			return picked ? [{ axis: x.axis, label: picked }] : [];
		});
		if (answers.every((a) => questions.find((x) => x.axis === a.axis)?.saved === a.label)) {
			onStatus({ tone: 'muted', text: `Already saved as ${words(questions.map(chosen))}.` });
			return;
		}
		onDecide(answers);
	}

	export function shortcut(index: number) {
		const q = questions.find((x) => !picks[x.axis]) ?? questions[0];
		if (!q || busy) return;
		const label = order(q)[index];
		if (label) choose(q, label);
	}

	function otherHelp(o: VocabularyLabel): string {
		return single
			? `${o.description} Saves it as this turn's label and opens the next turn.`
			: `${o.description} Saves once both questions are answered.`;
	}

	function disputedHelp(label: string): string {
		return single
			? `Saves ${wordFor(label)} as this turn's label and opens the next turn.`
			: `Chooses ${wordFor(label)}; saves once both questions are answered.`;
	}
</script>

<div class="eb-wrap" bind:offsetHeight={height}>
	<section class="eb edge" aria-label="Which is right?" data-testid="decision-bar">
		<div class="band-hd band-hd--static">
			<span class="machine eb-ask">Which is right?</span>
			{#if suggests}
				<Tooltip
					text="Jev's guess at how you reacted. It is a suggestion only; no answer is chosen for you."
				>
					{#snippet child({ props })}
						<span {...props} class="eb-suggest" data-testid="prelabel-suggestion"
							>{suggestionText}</span
						>
					{/snippet}
				</Tooltip>
			{/if}
			<span class="band-hd__meta eb-meta">
				<span
					class="eb-status"
					role="status"
					data-tone={line?.tone ?? 'muted'}
					data-testid="decision-status"
					>{#if line}{line.text}{/if}{#if line?.goBack !== undefined}{@const back = line.goBack}
						<button type="button" class="eb-back" onclick={() => onGoBack(back)}>Go back</button
						>{/if}</span
				>
			</span>
		</div>
		<div class="band-body eb-body">
			{#each questions as q, qi (q.axis)}
				<div
					class="eb-row"
					role="group"
					aria-label={single ? 'Which is right?' : ROW_LABEL[q.axis]}
				>
					{#if !single}<span class="machine eb-q">{ROW_LABEL[q.axis]}</span>{/if}
					{#each q.candidates as c (c.label)}
						<Tooltip text={disputedHelp(c.label)}>
							{#snippet child({ props })}
								<Button
									{...props}
									variant="default"
									class="eb-answer"
									data-label={c.label}
									aria-pressed={chosen(q) === c.label}
									disabled={busy}
									onclick={() => choose(q, c.label)}
								>
									{wordFor(c.label)}<span class="eb-tag">{c.tag}</span>
								</Button>
							{/snippet}
						</Tooltip>
					{/each}
					<span class="eb-else">Something else:</span>
					{#each q.others as o (o.label)}
						<Tooltip text={otherHelp(o)}>
							{#snippet child({ props })}
								<Button
									{...props}
									variant="outline"
									class="eb-answer"
									data-label={o.label}
									aria-pressed={chosen(q) === o.label}
									disabled={busy}
									onclick={() => choose(q, o.label)}
								>
									{wordFor(o.label)}
								</Button>
							{/snippet}
						</Tooltip>
					{/each}
					{#if qi === questions.length - 1}
						<Popover.Root bind:open={noteOpen}>
							<Tooltip text="Add a note that is saved with your answer">
								{#snippet child({ props: tip })}
									<Popover.Trigger>
										{#snippet child({ props: trigger })}
											<Button {...tip} {...trigger} variant="ghost" class="eb-note">
												<MessageSquarePlus aria-hidden="true" /><span class="eb-swap"
													><span class:eb-off={pending}>Add note</span><span class:eb-off={!pending}
														>Note added</span
													></span
												>
											</Button>
										{/snippet}
									</Popover.Trigger>
								{/snippet}
							</Tooltip>
							<Popover.Content side="top" align="end" class="eb-pop">
								<label class="eb-note-field">
									<span class="machine eb-pop-title">Note, saved with your answer</span>
									<textarea bind:value={note} rows="4"></textarea>
								</label>
								<div class="eb-note-done">
									<Button variant="outline" size="sm" onclick={() => (noteOpen = false)}
										>Done</Button
									>
								</div>
							</Popover.Content>
						</Popover.Root>
					{/if}
				</div>
			{/each}
		</div>
	</section>
</div>

<style>
	.eb-wrap {
		container: eb / inline-size;
		position: sticky;
		bottom: 0;
		z-index: 5;
	}
	.eb {
		--edge: var(--edge-md);
		--edge-pad: 0px;
		display: flex;
		flex-direction: column;
	}
	.eb-ask {
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.eb-suggest {
		min-width: 24ch;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-2);
		cursor: help;
	}
	.eb-meta {
		min-width: 0;
		overflow-x: auto;
	}
	.eb-status {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.eb-status[data-tone='success'] {
		color: var(--status-success-fg);
	}
	.eb-status[data-tone='danger'] {
		color: var(--status-danger-fg);
	}
	.eb-back {
		margin-left: var(--space-2);
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--interactive-active-fg);
		cursor: pointer;
	}
	.eb-back:hover {
		text-decoration: underline;
	}
	.eb-back:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.eb-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.eb-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
	}
	.eb-q {
		flex: 0 0 100%;
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.eb-else {
		margin-left: var(--space-2);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.eb-tag {
		margin-left: var(--space-1);
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		font-weight: var(--weight-body);
		letter-spacing: normal;
		text-transform: none;
		color: var(--text-2);
	}
	.eb :global(.eb-answer[aria-pressed='true']) {
		--aug-border-bg: var(--interactive-active-border);
		background: var(--interactive-active-bg);
		color: var(--interactive-active-fg);
		text-shadow: var(--glow-cyan);
	}
	.eb :global(.eb-note) {
		margin-left: auto;
	}
	.eb-swap {
		display: inline-grid;
	}
	.eb-swap > span {
		grid-area: 1 / 1;
	}
	.eb-off {
		visibility: hidden;
	}
	:global([data-slot='popover-content'].eb-pop) {
		inline-size: min(34rem, calc(100vw - 2 * var(--space-4)));
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.eb-pop-title {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.eb-note-field {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.eb-note-field textarea {
		inline-size: 100%;
		padding: var(--space-2);
		background: var(--surface-bg);
		border: var(--hairline) solid var(--border-default);
		color: var(--text-1);
		font: inherit;
		font-size: var(--text-sm);
		resize: vertical;
	}
	.eb-note-field textarea:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -1px;
	}
	.eb-note-done {
		display: flex;
		justify-content: flex-end;
	}
	@container eb (max-width: 640px) {
		.eb-else {
			flex: 0 0 100%;
			margin-left: 0;
		}
	}
</style>
