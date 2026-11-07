<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import {
		confirmRelayedLabel,
		getEvalVocabulary,
		LabelWriteError,
		listEvalLabels,
		postGoldLabels,
		withdrawLabel
	} from '$lib/api/eval';
	import type { EvalLabelDto, LabelAxis, LabelSubject, VocabularyAxis } from '$lib/api/eval';
	import { createSessionNote, getSessionNotes } from '$lib/comms/sessionNotes';
	import type { SessionNoteDto } from '$lib/comms/sessionNotes';
	import { Button } from '$lib/components/ui/button';
	import { Field } from '$lib/components/ui/field';
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
	import EvalChoiceGroup from './EvalChoiceGroup.svelte';
	import LabelHistoryList from './LabelHistoryList.svelte';
	import {
		axisWord,
		currentGold,
		notesForSubject,
		optionsFor,
		planSave,
		wordFor
	} from './labelControl';

	interface Props {
		subject: LabelSubject;
		axes: LabelAxis[];
		context: 'turn_card' | 'session_summary';
		sessionId?: string;
	}

	let { subject, axes, context, sessionId }: Props = $props();

	let vocabulary = $state<VocabularyAxis[]>([]);
	let labels = $state<EvalLabelDto[]>([]);
	let verdict = $state<Record<string, unknown> | null>(null);
	let selection = $state<Record<string, string | null>>({});
	let note = $state('');
	let busy = $state(false);
	let message = $state<string | null>(null);
	let saved = $state<{ at: string; via: 'labels' | 'note' } | null>(null);
	let notes = $state<SessionNoteDto[]>([]);
	let notesError = $state(false);
	let loadError = $state<string | null>(null);

	const regionLabel = $derived(
		subject.kind === 'turn'
			? `Label for turn ${subject.turnId}`
			: `Label for session ${subject.sessionId}`
	);

	const notebookId = $derived(subject.kind === 'session_end' ? subject.sessionId : sessionId);
	const plan = $derived(planSave(selection, labels, note, notebookId !== undefined));
	const shownNotes = $derived(
		notesForSubject(notes, subject.kind === 'turn' ? subject.turnId : null)
	);
	const notebookHref = $derived(
		notebookId === undefined
			? null
			: resolve(`sessions/${encodeURIComponent(notebookId)}/comms` as `sessions/${string}/comms`)
	);
	const holdout = $derived(labels.some((l) => l.split === 'holdout'));

	function describeError(e: unknown): string {
		if (e instanceof LabelWriteError && e.status === 403)
			return 'Labels can only be saved from the mike UI';
		if (e instanceof LabelWriteError && e.status === 409) return e.message;
		return e instanceof Error ? e.message : String(e);
	}

	async function refresh() {
		const response = await listEvalLabels(subject);
		labels = response.labels;
		verdict = response.verdict_now;
		selection = Object.fromEntries(
			axes.map((axis) => [axis, currentGold(labels, axis)?.label ?? null])
		);
	}

	async function loadNotes() {
		if (notebookId === undefined) return;
		try {
			const loaded: unknown = await getSessionNotes(notebookId, { tag: 'operator', limit: 100 });
			notes = Array.isArray(loaded) ? (loaded as SessionNoteDto[]) : [];
			notesError = !Array.isArray(loaded);
		} catch {
			notesError = true;
		}
	}

	onMount(() => {
		void loadNotes();
		void (async () => {
			try {
				vocabulary = await getEvalVocabulary();
				await refresh();
			} catch (e) {
				loadError = describeError(e);
			}
		})();
	});

	function clockNow(): string {
		return new Date().toLocaleTimeString([], {
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
			hour12: false
		});
	}

	async function run(action: () => Promise<unknown>, via: 'labels' | 'note' | null) {
		busy = true;
		message = null;
		saved = null;
		try {
			await action();
			await refresh();
			note = '';
			if (via) saved = { at: clockNow(), via };
		} catch (e) {
			message = describeError(e);
		} finally {
			busy = false;
		}
	}

	function save() {
		if (plan.kind === 'labels') {
			const { entries } = plan;
			void run(() => postGoldLabels(subject, entries, note.trim() || null, context), 'labels');
		} else if (plan.kind === 'note' && notebookId !== undefined) {
			const { title, body } = plan;
			const id = notebookId;
			const data = {
				source: 'label_control',
				context,
				...(subject.kind === 'turn' ? { turn_id: subject.turnId } : {})
			};
			void run(async () => {
				await createSessionNote(id, { kind: 'note', title, body, tags: ['operator'], data });
				await loadNotes();
			}, 'note');
		}
	}

	function choose(axis: string, label: string) {
		selection = { ...selection, [axis]: label };
		saved = null;
	}

	const OUTCOME_BADGE: Record<string, BadgeVariant> = {
		success: 'chip-active',
		partial: 'chip-warning',
		failure: 'chip-error'
	};

	const verdictOutcome = $derived(String(verdict?.['outcome'] ?? 'unknown'));
	const verdictMeta = $derived(
		[
			verdict?.['judge_model'] ? String(verdict['judge_model']) : null,
			verdict?.['rubric_version'] ? `rubric ${String(verdict['rubric_version'])}` : null,
			verdict?.['bundle_id'] ? `bundle #${String(verdict['bundle_id'])}` : null
		]
			.filter((part): part is string => part !== null)
			.join(' · ')
	);

	const status = $derived.by((): { kind: 'error' | 'ok' | 'hint' | 'none'; text: string } => {
		if (message) return { kind: 'error', text: message };
		if (saved) {
			return {
				kind: 'ok',
				text: saved.via === 'note' ? `Note saved ${saved.at}` : `Saved ${saved.at}`
			};
		}
		if (plan.kind !== 'nothing') return { kind: 'none', text: '' };
		if (plan.reason === 'choose') return { kind: 'hint', text: 'Choose a label or write a note.' };
		if (plan.reason === 'note-needs-label') {
			return { kind: 'hint', text: 'Choose a label to save the note.' };
		}
		return { kind: 'none', text: '' };
	});
</script>

<section class="glc" aria-label={regionLabel}>
	<div class="glc-head">
		<span class="glc-title machine">Your label</span>
		{#if verdict}
			<span class="glc-judge">
				<span>Judge</span>
				<Badge variant={OUTCOME_BADGE[verdictOutcome] ?? 'chip-muted'}
					>{wordFor(verdictOutcome)}</Badge
				>
				{#if verdictMeta}<span class="glc-meta">{verdictMeta}</span>{/if}
			</span>
		{/if}
	</div>
	{#if loadError}
		<p class="glc-error" role="status">Couldn't load the label: {loadError}</p>
	{/if}
	<div class="glc-axes">
		{#each axes as axis (axis)}
			<EvalChoiceGroup
				label={axisWord(axis)}
				choices={optionsFor(vocabulary, axis).map((option) => ({
					value: option.label,
					label: wordFor(option.label),
					description: option.description
				}))}
				value={selection[axis] ?? null}
				onChange={(label) => choose(axis, label)}
			/>
		{/each}
	</div>
	<div class="glc-note">
		<Field
			multiline
			rows={2}
			label="Note"
			placeholder="Why this label"
			bind:value={note}
			oninput={() => (saved = null)}
		/>
	</div>
	<div class="glc-save">
		<Button size="sm" disabled={busy || plan.kind === 'nothing'} onclick={save}>Save</Button>
		<span
			class="glc-status"
			data-kind={status.kind}
			role={status.kind === 'error' ? 'alert' : status.kind === 'ok' ? 'status' : undefined}
			>{status.text}{#if status.kind === 'ok' && saved?.via === 'note' && notebookHref}
				<a class="glc-link" href={notebookHref}>session notebook</a>{/if}</span
		>
	</div>
	{#if shownNotes.length > 0}
		<div class="glc-notes">
			<span class="glc-title machine">Your notes</span>
			<ul class="glc-note-list">
				{#each shownNotes as saved_note (saved_note.note_uuid)}
					<li class="glc-note-row">
						<p class="glc-note-body">{saved_note.body}</p>
						<time class="glc-faint" datetime={saved_note.created_at}
							>{new Date(saved_note.created_at).toLocaleString()}</time
						>
					</li>
				{/each}
			</ul>
		</div>
	{/if}
	{#if notesError}
		<p class="glc-faint">Notes could not be loaded.</p>
	{/if}
	{#if holdout}
		<p class="glc-faint">
			Holdout labels are for measurement only; mike never uses them as context.
		</p>
	{/if}
	{#if labels.length > 0}
		<LabelHistoryList
			{labels}
			{busy}
			onWithdraw={(id) => void run(() => withdrawLabel(id), null)}
			onConfirm={(id) => void run(() => confirmRelayedLabel(id), null)}
		/>
	{/if}
</section>

<style>
	.glc {
		--ecc-label-w: 11rem;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		font-size: var(--text-xs);
	}
	.glc-head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
	}
	.glc-title {
		font-size: var(--text-micro);
		color: var(--text-3);
	}
	.glc-judge {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-2);
	}
	.glc-meta {
		font-family: var(--font-mono);
		color: var(--text-3);
	}
	.glc-axes {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.glc-note {
		max-width: 72ch;
	}
	.glc-save {
		display: flex;
		gap: var(--space-3);
		align-items: center;
		min-height: var(--control-h);
	}
	.glc-status {
		min-height: 1lh;
		color: var(--text-3);
	}
	.glc-status[data-kind='error'] {
		color: var(--status-danger-fg);
	}
	.glc-status[data-kind='ok'] {
		color: var(--status-success-fg);
	}
	.glc-error {
		margin: 0;
		color: var(--status-danger-fg);
	}
	.glc-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
	}
	.glc-notes {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.glc-note-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		max-width: 72ch;
	}
	.glc-note-row {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.glc-note-body {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		color: var(--text-1);
	}
	.glc-faint {
		margin: 0;
		color: var(--text-faint);
	}
</style>
