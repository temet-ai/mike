<script lang="ts">
	import { ApiError } from '$lib/api';
	import { formatRelative } from '$lib/time';
	import { shortSessionId } from '$lib/format';
	import { resolve } from '$app/paths';
	import { roleTokens } from './roleTokens';
	import Badge from '$lib/components/shared/Badge.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import {
		getProjectNotes,
		NOTE_KIND_OPTIONS,
		NOTE_TASK_STATUS_OPTIONS
	} from '$lib/comms/sessionNotes';
	import type { ProjectSessionNoteDto } from '$lib/comms/sessionNotes';

	interface Props {
		projectPath: string;
	}

	const { projectPath }: Props = $props();

	let roleFilter = $state('');
	let kindFilter = $state('');
	let statusFilter = $state('');
	let tagFilter = $state('');
	let qFilter = $state('');

	let notes = $state<ProjectSessionNoteDto[]>([]);
	let totalCount = $state(0);
	let loading = $state(true);
	let loadError = $state<string | null>(null);

	let fetchSeq = 0;
	let debounceTimer: ReturnType<typeof setTimeout> | null = null;

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

	async function load(project: string): Promise<void> {
		const seq = ++fetchSeq;
		loading = true;
		loadError = null;
		try {
			const res = await getProjectNotes(project, {
				role: roleFilter.trim() || undefined,
				kind: kindFilter.trim() || undefined,
				status: statusFilter.trim() || undefined,
				tag: tagFilter.trim() || undefined,
				q: qFilter.trim() || undefined,
				limit: 200
			});
			if (seq !== fetchSeq) return;
			notes = res.notes;
			totalCount = res.total_count;
		} catch (err) {
			if (seq !== fetchSeq) return;
			loadError = err instanceof ApiError ? err.message : String(err);
		} finally {
			if (seq === fetchSeq) loading = false;
		}
	}

	function scheduleLoad(project: string): void {
		if (debounceTimer !== null) clearTimeout(debounceTimer);
		debounceTimer = setTimeout(() => void load(project), 250);
	}

	let initializedFor: string | null = null;

	$effect(() => {
		const project = projectPath;
		void roleFilter;
		void kindFilter;
		void statusFilter;
		void tagFilter;
		void qFilter;
		if (initializedFor !== project) {
			initializedFor = project;
			void load(project);
			return;
		}
		scheduleLoad(project);
		return () => {
			if (debounceTimer !== null) clearTimeout(debounceTimer);
		};
	});

	function retry(): void {
		void load(projectPath);
	}

	function clearFilters(): void {
		roleFilter = '';
		kindFilter = '';
		statusFilter = '';
		tagFilter = '';
		qFilter = '';
	}

	const hasActiveFilter = $derived(
		roleFilter.trim().length > 0 ||
			kindFilter.trim().length > 0 ||
			statusFilter.trim().length > 0 ||
			tagFilter.trim().length > 0 ||
			qFilter.trim().length > 0
	);

	function applyObserverFindings(): void {
		roleFilter = 'observer';
		kindFilter = 'finding';
		statusFilter = '';
		tagFilter = '';
		qFilter = '';
	}

	function applyOpenTasks(): void {
		roleFilter = '';
		kindFilter = 'task';
		statusFilter = 'open';
		tagFilter = '';
		qFilter = '';
	}

</script>

<section class="pnb" data-slot="project-notebook" aria-label="Project notebook">
	<div class="pnb-presets" role="group" aria-label="Notebook presets">
		<button
			type="button"
			class="pnb-preset"
			data-slot="preset-observer-findings"
			aria-pressed={roleFilter === 'observer' && kindFilter === 'finding'}
			onclick={applyObserverFindings}
		>
			Observer findings
		</button>
		<button
			type="button"
			class="pnb-preset"
			data-slot="preset-open-tasks"
			aria-pressed={kindFilter === 'task' && statusFilter === 'open'}
			onclick={applyOpenTasks}
		>
			Open tasks
		</button>
		<button
			type="button"
			class="pnb-preset"
			data-slot="preset-clear"
			onclick={clearFilters}
		>
			All notes
		</button>
	</div>
	<div class="pnb-filters" role="group" aria-label="Notebook filters">
		<label class="pnb-field">
			<span class="pnb-label">Role</span>
			<input
				class="pnb-input"
				type="text"
				list="pnb-role-hint"
				bind:value={roleFilter}
				placeholder="e.g. observer"
			/>
			<datalist id="pnb-role-hint">
				<option value="observer"></option>
				<option value="coordinator"></option>
				<option value="rust-developer"></option>
				<option value="frontend-developer"></option>
				<option value="ui-designer"></option>
				<option value="db-dev"></option>
			</datalist>
		</label>
		<label class="pnb-field">
			<span class="pnb-label">Kind</span>
			<input class="pnb-input" type="text" list="pnb-kind-hint" bind:value={kindFilter} />
			<datalist id="pnb-kind-hint">
				{#each NOTE_KIND_OPTIONS as k (k)}
					<option value={k}></option>
				{/each}
			</datalist>
		</label>
		<label class="pnb-field">
			<span class="pnb-label">Status</span>
			<input class="pnb-input" type="text" list="pnb-status-hint" bind:value={statusFilter} />
			<datalist id="pnb-status-hint">
				{#each NOTE_TASK_STATUS_OPTIONS as s (s)}
					<option value={s}></option>
				{/each}
			</datalist>
		</label>
		<label class="pnb-field">
			<span class="pnb-label">Tag</span>
			<input class="pnb-input" type="text" bind:value={tagFilter} placeholder="e.g. blocker" />
		</label>
		<label class="pnb-field pnb-field--search">
			<span class="pnb-label">Search</span>
			<input class="pnb-input" type="text" bind:value={qFilter} placeholder="title or body text" />
		</label>
		{#if hasActiveFilter}
			<button type="button" class="pnb-clear" onclick={clearFilters}>Clear filters</button>
		{/if}
	</div>

	<div class="pnb-summary" aria-live="polite">
		{#if loading}
			Loading notebook
		{:else if loadError}
			&nbsp;
		{:else}
			{totalCount} note{totalCount === 1 ? '' : 's'}
			{hasActiveFilter ? 'matching these filters' : `across ${projectPath}`}
		{/if}
	</div>

	{#if loadError}
		<LoadErrorBanner title="Couldn't load the project notebook" detail={loadError} onRetry={retry} />
	{:else if !loading && notes.length === 0}
		<p class="pnb-empty">
			{hasActiveFilter ? 'No notes match these filters.' : 'No notes recorded in this project yet.'}
		</p>
	{:else}
		<ul class="pnb-list" role="list">
			{#each notes as note (note.note_uuid)}
				{@const tokens = roleTokens(note.session_role)}
				<li class="pnb-row" data-slot="project-note-row">
					<div class="pnb-row__head">
						<a
							class="pnb-row__session"
							href={resolve(
								`/sessions/${encodeURIComponent(note.session_uuid)}` as `/sessions/${string}`
							)}
							style:color={tokens.fg}
						>
							{note.session_name ?? shortSessionId(note.session_uuid)}
						</a>
						{#if note.session_role}
							<span class="pnb-row__role">{note.session_role}</span>
						{/if}
						<Badge variant={kindVariant(note.kind)}>{note.kind}</Badge>
						{#if note.kind === 'task' && note.status}
							<Badge variant={statusVariant(note.status)}>{note.status}</Badge>
						{/if}
						<span class="pnb-row__time" title={note.created_at}>
							{formatRelative(note.created_at)}
						</span>
					</div>
					<span class="pnb-row__title">{note.title}</span>
					{#if note.body}
						<p class="pnb-row__body">{note.body}</p>
					{/if}
					{#if note.tags.length > 0}
						<div class="pnb-row__tags">
							{#each note.tags as tag (tag)}
								<Badge variant="tag-default">{tag}</Badge>
							{/each}
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.pnb {
		display: flex;
		min-height: 0;
		flex: 1;
		flex-direction: column;
		gap: 12px;
		overflow-y: auto;
		padding: 16px 20px 26px;
	}

	.pnb-filters {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 10px;
	}

	.pnb-field {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.pnb-field--search {
		flex: 1;
		min-width: 180px;
	}

	.pnb-label {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.pnb-input {
		min-width: 140px;
		padding: 5px 8px;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--surface-card);
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
	}

	.pnb-field--search .pnb-input {
		width: 100%;
		min-width: 0;
	}

	.pnb-input:focus-visible {
		outline: none;
		border-color: var(--interactive-active-border);
		box-shadow: 0 0 0 2px var(--ring);
	}

	.pnb-clear {
		align-self: flex-end;
		padding: 5px 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
		background: transparent;
		color: var(--text-3);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		cursor: pointer;
	}

	.pnb-clear:hover {
		color: var(--text-strong);
		border-color: var(--interactive-active-border);
	}

	.pnb-clear:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}

	.pnb-summary {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.pnb-empty {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-3);
	}

	.pnb-list {
		display: flex;
		flex-direction: column;
		gap: 10px;
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.pnb-row {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px 14px;
		background: var(--surface-card);
	}

	.pnb-row__head {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
	}

	.pnb-row__session {
		font-family: var(--font-display);
		font-weight: 700;
		font-size: 13px;
		text-decoration: none;
	}

	.pnb-row__session:hover {
		text-decoration: underline;
	}

	.pnb-row__session:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.pnb-presets {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin-block-end: var(--space-3);
	}

	.pnb-preset {
		border: var(--border-width) solid var(--edge-muted);
		background: var(--surface-raised);
		color: var(--text-secondary);
		font: inherit;
		padding: var(--space-1) var(--space-3);
		cursor: pointer;
	}

	.pnb-preset[aria-pressed='true'] {
		color: var(--text-primary);
		border-color: var(--edge-accent);
	}

	.pnb-row__role {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.pnb-row__time {
		margin-left: auto;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-faint);
	}

	.pnb-row__title {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 600;
		color: var(--text-1);
		overflow-wrap: break-word;
	}

	.pnb-row__body {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: 12.5px;
		line-height: 1.55;
		color: var(--text-2);
	}

	.pnb-row__tags {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}
</style>
