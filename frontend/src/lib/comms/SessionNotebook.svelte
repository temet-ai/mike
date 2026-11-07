<script lang="ts">
	import { getContext } from 'svelte';
	import { Info, Plus } from '@lucide/svelte';
	import { ApiError } from '$lib/api';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { formatRelative } from '$lib/time';
	import { createRetryBackoff } from '$lib/retryBackoff';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Badge from '$lib/components/shared/Badge.svelte';
	import {
		createSessionNote,
		getSessionNotes,
		NOTE_KIND_OPTIONS,
		NOTE_TASK_STATUS_OPTIONS
	} from './sessionNotes';
	import type { SessionNoteDto } from './sessionNotes';

	interface Props {
		sessionId: string;
	}

	const { sessionId }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionKey = $derived(`session:${sessionId}:notebook-open`);
	let localExpanded = $state(false);
	const expanded = $derived(collapseStore ? collapseStore.isExpanded(sectionKey) : localExpanded);

	function toggle(): void {
		if (collapseStore) collapseStore.toggle(sectionKey);
		else localExpanded = !localExpanded;
	}

	let notes = $state<SessionNoteDto[]>([]);
	let loading = $state(false);
	let loadError = $state<string | null>(null);
	let attemptedFor = $state<string | null>(null);

	const backoff = createRetryBackoff();

	async function load(id: string): Promise<void> {
		backoff.cancel();
		loading = true;
		loadError = null;
		try {
			const result = await getSessionNotes(id, { limit: 100 });
			if (id !== sessionId) return;
			notes = result;
			backoff.succeeded();
		} catch (err) {
			if (id !== sessionId) return;
			loadError = err instanceof ApiError ? err.message : String(err);
			backoff.failed(() => {
				if (id === sessionId) void load(id);
			});
		} finally {
			loading = false;
		}
	}

	function retry(): void {
		backoff.reset();
		void load(sessionId);
	}

	$effect(() => {
		const id = sessionId;
		if (!expanded) return;
		if (attemptedFor === id) return;
		attemptedFor = id;
		backoff.reset();
		void load(id);
	});

	$effect(() => backoff.cancel);

	let composerOpen = $state(false);
	let draftKind = $state('note');
	let draftTitle = $state('');
	let draftBody = $state('');
	let draftTags = $state('');
	let draftStatus = $state('open');
	let submitting = $state(false);
	let submitError = $state<string | null>(null);
	let titleInputEl: HTMLInputElement | undefined = $state();

	const canSubmit = $derived(!submitting && draftTitle.trim().length > 0);

	function resetComposer(): void {
		draftKind = 'note';
		draftTitle = '';
		draftBody = '';
		draftTags = '';
		draftStatus = 'open';
	}

	function parseTags(raw: string): string[] {
		return raw
			.split(',')
			.map((t) => t.trim())
			.filter((t) => t.length > 0);
	}

	function kindVariant(kind: string): 'chip-muted' | 'chip-analyzed' | 'chip-warning' {
		if (kind === 'finding' || kind === 'decision') return 'chip-analyzed';
		if (kind === 'question') return 'chip-warning';
		return 'chip-muted';
	}

	function statusVariant(
		status: string
	): 'chip-muted' | 'chip-active' | 'chip-warning' | 'chip-analyzed' {
		if (status === 'done') return 'chip-active';
		if (status === 'blocked') return 'chip-warning';
		if (status === 'in_progress') return 'chip-analyzed';
		return 'chip-muted';
	}

	async function submit(): Promise<void> {
		if (!canSubmit) return;
		submitting = true;
		submitError = null;
		try {
			const created = await createSessionNote(sessionId, {
				kind: draftKind.trim() || 'note',
				title: draftTitle.trim(),
				body: draftBody.trim() || undefined,
				status: draftKind.trim() === 'task' ? draftStatus : undefined,
				tags: parseTags(draftTags)
			});
			notes = [created, ...notes];
			resetComposer();
			composerOpen = false;
		} catch (err) {
			submitError = err instanceof ApiError ? err.message : String(err);
		} finally {
			submitting = false;
		}
	}

	$effect(() => {
		if (composerOpen) titleInputEl?.focus();
	});
</script>

<section class="snb" data-slot="session-notebook" data-augmented-ui="tl-clip br-clip border">
	<div class="snb-head">
		<button type="button" class="snb-toggle" aria-expanded={expanded} onclick={toggle}>
			<span class="snb-title">Notebook</span>
			{#if notes.length > 0}
				<span class="snb-count">{notes.length}</span>
			{/if}
		</button>
		<Tooltip
			text="This session's own notebook — findings, tasks, decisions and questions it has recorded, queryable across the project."
		>
			<Info size={12} aria-hidden="true" class="snb-help" />
		</Tooltip>
	</div>

	{#if expanded}
		<div class="snb-body">
			<div class="snb-composer">
				<button
					type="button"
					class="snb-add-toggle"
					onclick={() => (composerOpen = !composerOpen)}
					aria-expanded={composerOpen}
				>
					<Plus size={12} aria-hidden="true" />
					Add note
				</button>

				{#if composerOpen}
					<form
						class="snb-form"
						data-augmented-ui="tl-clip br-clip border"
						onsubmit={(e) => {
							e.preventDefault();
							void submit();
						}}
					>
						<div class="snb-form-row">
							<label class="snb-field snb-field--kind">
								<span class="snb-label">Kind</span>
								<input
									class="snb-input"
									type="text"
									list="snb-kind-options"
									bind:value={draftKind}
									disabled={submitting}
								/>
								<datalist id="snb-kind-options">
									{#each NOTE_KIND_OPTIONS as k (k)}
										<option value={k}></option>
									{/each}
								</datalist>
							</label>
							{#if draftKind.trim() === 'task'}
								<label class="snb-field snb-field--status">
									<span class="snb-label">Status</span>
									<select class="snb-input" bind:value={draftStatus} disabled={submitting}>
										{#each NOTE_TASK_STATUS_OPTIONS as s (s)}
											<option value={s}>{s}</option>
										{/each}
									</select>
								</label>
							{/if}
						</div>
						<label class="snb-field">
							<span class="snb-label">Title <span class="snb-hint">(required)</span></span>
							<input
								class="snb-input"
								type="text"
								bind:value={draftTitle}
								bind:this={titleInputEl}
								placeholder="what this note is about"
								required
								aria-required="true"
								disabled={submitting}
							/>
						</label>
						<label class="snb-field">
							<span class="snb-label">Body</span>
							<textarea
								class="snb-textarea"
								bind:value={draftBody}
								rows="3"
								placeholder="the detail — full text, never trimmed"
								disabled={submitting}></textarea>
						</label>
						<label class="snb-field">
							<span class="snb-label"
								>Tags <span class="snb-hint">(optional, comma-separated)</span></span
							>
							<input
								class="snb-input"
								type="text"
								bind:value={draftTags}
								placeholder="blocker, db"
								disabled={submitting}
							/>
						</label>

						{#if submitError}
							<p class="snb-error" role="alert">{submitError}</p>
						{/if}

						<div class="snb-actions">
							<button
								type="button"
								class="snb-cancel"
								onclick={() => {
									resetComposer();
									composerOpen = false;
								}}
								disabled={submitting}
							>
								Cancel
							</button>
							<button
								type="submit"
								class="snb-submit"
								data-augmented-ui="tl-clip br-clip border"
								disabled={!canSubmit}
							>
								{submitting ? 'Saving' : 'Save note'}
							</button>
						</div>
					</form>
				{/if}
			</div>

			{#if loading && notes.length === 0}
				<p class="snb-status">Loading notebook</p>
			{:else if loadError}
				<LoadErrorBanner
					title="Couldn't load this session's notebook"
					detail={loadError}
					onRetry={retry}
				/>
			{:else if notes.length === 0}
				<p class="snb-status">No notes recorded yet.</p>
			{:else}
				<ul class="snb-list" role="list">
					{#each notes as note (note.note_uuid)}
						<li class="snb-row" data-slot="session-note-row">
							<div class="snb-row__head">
								<Badge variant={kindVariant(note.kind)}>{note.kind}</Badge>
								{#if note.kind === 'task' && note.status}
									<Badge variant={statusVariant(note.status)}>{note.status}</Badge>
								{/if}
								<span class="snb-row__title">{note.title}</span>
								<span class="snb-row__time" title={note.created_at}>
									{formatRelative(note.created_at)}
								</span>
							</div>
							{#if note.body}
								<p class="snb-row__body">{note.body}</p>
							{/if}
							{#if note.tags.length > 0}
								<div class="snb-row__tags">
									{#each note.tags as tag (tag)}
										<Badge variant="tag-default">{tag}</Badge>
									{/each}
								</div>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{/if}
</section>

<style>
	.snb {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		background: var(--surface-card);
	}

	.snb-head {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 10px 14px;
	}

	.snb-toggle {
		display: inline-flex;
		flex: 1;
		align-items: center;
		gap: 8px;
		min-width: 0;
		border: none;
		background: transparent;
		padding: 0;
		cursor: pointer;
		text-align: left;
	}

	.snb-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.snb-title {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.snb-count {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-faint);
	}

	:global(.snb-help) {
		flex: none;
		color: var(--text-faint);
	}

	.snb-body {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 0 14px 14px;
	}

	.snb-composer {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.snb-add-toggle {
		display: inline-flex;
		align-self: flex-start;
		align-items: center;
		gap: 5px;
		padding: 4px 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		cursor: pointer;
	}

	.snb-add-toggle:hover {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.snb-add-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.snb-form {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 10px 12px;
		background: var(--surface-2);
	}

	.snb-form-row {
		display: flex;
		gap: 8px;
	}

	.snb-field {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 3px;
	}

	.snb-field--kind,
	.snb-field--status {
		flex: none;
		width: 140px;
	}

	.snb-label {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		color: var(--text-3);
	}

	.snb-hint {
		font-weight: 400;
		text-transform: none;
	}

	.snb-input,
	.snb-textarea {
		padding: 5px 7px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-card);
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
	}

	.snb-textarea {
		resize: vertical;
		min-height: 56px;
		line-height: 1.5;
	}

	.snb-input:focus-visible,
	.snb-textarea:focus-visible {
		outline: 2px solid var(--status-info-border);
		outline-offset: 1px;
	}

	.snb-error {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	.snb-actions {
		display: flex;
		gap: 8px;
	}

	.snb-cancel {
		padding: 5px 14px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-card);
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		cursor: pointer;
	}

	.snb-cancel:hover:not(:disabled) {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.snb-cancel:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.snb-cancel:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.snb-submit {
		--edge: var(--edge-sm, 4px);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border);
		padding: 5px 14px;
		background: var(--status-steering-bg);
		color: var(--status-steering-fg);
		font-family: var(--font-display);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		cursor: pointer;
	}

	.snb-submit:hover:not(:disabled) {
		background: var(--status-steering-border);
	}

	.snb-submit:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}

	.snb-status {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.snb-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.snb-row {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: 10px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 8px 10px;
		background: var(--surface-2);
	}

	.snb-row__head {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
	}

	.snb-row__title {
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-1);
	}

	.snb-row__time {
		flex: none;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-faint);
	}

	.snb-row__body {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: 12.5px;
		line-height: 1.55;
		color: var(--text-2);
	}

	.snb-row__tags {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}
</style>
