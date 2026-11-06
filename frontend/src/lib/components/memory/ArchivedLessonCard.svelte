<script lang="ts">
	import { Undo2 } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { describeArchivedReason, type ArchivedLesson } from '$lib/api/memoryDecisions';
	import { formatRelative, formatClockAbsolute } from '$lib/time';
	import { shortSessionId } from '$lib/format';

	interface Props {
		lesson: ArchivedLesson;
		onRestore?: (lesson: ArchivedLesson) => void;
		restoring?: boolean;
		restoreError?: string | null;
		restored?: boolean;
	}

	let {
		lesson,
		onRestore,
		restoring = false,
		restoreError = null,
		restored = false
	}: Props = $props();

	const reasonInfo = $derived(describeArchivedReason(lesson.archived_reason));
	const canRestore = $derived(!!onRestore && !restored);
	const archivedTimestamp = $derived(lesson.archived_at ?? lesson.created_at);

	function shortProject(p: string | null): string {
		if (!p) return '—';
		const segs = p.split('/').filter(Boolean);
		return segs.length > 0 ? (segs[segs.length - 1] ?? p) : p;
	}

	function handleRestore(): void {
		onRestore?.(lesson);
	}
</script>

<article class="alc-card" data-augmented-ui="tl-clip br-clip border" data-lesson-id={lesson.id}>
	<header class="alc-head">
		<span class="badge-nx badge-{reasonInfo.tone}">{reasonInfo.label}</span>
		<span
			class="alc-when font-mono text-micro tabular-nums text-[var(--text-faint)]"
			title={formatClockAbsolute(archivedTimestamp)}
		>
			{formatRelative(archivedTimestamp)}
		</span>
		{#if canRestore}
			<button
				type="button"
				class="alc-restore"
				disabled={restoring}
				onclick={handleRestore}
				aria-label="Restore this lesson from the archive"
			>
				<Undo2 size={12} aria-hidden="true" />
				{restoring ? 'Restoring…' : 'Restore'}
			</button>
		{:else if restored}
			<span class="alc-restored">
				<Undo2 size={12} aria-hidden="true" />
				Restored
			</span>
		{/if}
	</header>

	{#if restoreError}
		<p class="alc-restore-error" role="alert">Couldn't restore: {restoreError}</p>
	{/if}

	<div class="alc-provenance" role="group" aria-label="Provenance">
		{#if lesson.project_path}
			<a
				class="alc-chip alc-chip--link"
				href={resolve(`/projects/${encodeURIComponent(lesson.project_path)}` as const)}
				title={lesson.project_path}
			>
				{shortProject(lesson.project_path)}
			</a>
		{/if}
		{#if lesson.session_uuid}
			<a
				class="alc-chip alc-chip--link"
				href={resolve(`/sessions/${encodeURIComponent(lesson.session_uuid)}` as const)}
				aria-label="Open session {lesson.session_uuid}"
			>
				session {shortSessionId(lesson.session_uuid)}
			</a>
		{/if}
		{#if lesson.topic}
			<span class="alc-chip">{lesson.topic}</span>
		{/if}
		{#if lesson.duplicate_of_id !== null}
			<span class="alc-chip">duplicate of lesson #{lesson.duplicate_of_id}</span>
		{/if}
	</div>

	<p class="alc-text">{lesson.text}</p>
</article>

<style>
	.alc-card {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		background: var(--surface-card);
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: calc(var(--edge) + 4px);
	}

	.alc-head {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.alc-when {
		margin-left: auto;
	}

	.alc-restore {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		border: 1px solid var(--border-default);
		background: var(--surface-2);
		color: var(--text-1);
		border-radius: 4px;
		padding: 3px 8px;
		font-size: var(--text-label);
		cursor: pointer;
	}
	.alc-restore:hover:not(:disabled) {
		border-color: var(--interactive-active-border);
		color: var(--text-strong);
	}
	.alc-restore:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.alc-restore:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.alc-restored {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		color: var(--status-success-fg);
		font-size: var(--text-label);
	}

	.alc-restore-error {
		margin: 0;
		font-size: var(--text-label);
		color: var(--status-danger-fg);
	}

	.alc-provenance {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.alc-chip {
		display: inline-flex;
		align-items: center;
		border-radius: 4px;
		background: var(--surface-2);
		color: var(--text-2);
		padding: 2px 6px;
		font-size: var(--text-label);
		font-family: var(--font-mono, monospace);
	}
	.alc-chip--link {
		color: var(--interactive-active-fg);
		text-decoration: none;
	}
	.alc-chip--link:hover {
		text-decoration: underline;
	}

	.alc-text {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
	}
</style>
