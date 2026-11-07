<script lang="ts">
	import { Undo2 } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import {
		describeActionKind,
		outcomeTone,
		isRestorableAction,
		type ConsolidationAction
	} from '$lib/api/memoryDecisions';
	import { formatRelative, formatClockAbsolute } from '$lib/time';
	import { shortSessionId } from '$lib/format';

	interface Props {
		action: ConsolidationAction;
		onRestore?: (action: ConsolidationAction) => void;
		restoring?: boolean;
		restoreError?: string | null;
		restored?: boolean;
	}

	let {
		action,
		onRestore,
		restoring = false,
		restoreError = null,
		restored = false
	}: Props = $props();

	const kindInfo = $derived(describeActionKind(action.action_kind));
	const canRestore = $derived(isRestorableAction(action) && !!onRestore && !restored);
	const hasCounterpart = $derived(action.counterpart_text !== null);
	const rankDetailEntries = $derived(action.rank_detail ? Object.entries(action.rank_detail) : []);

	function shortProject(p: string | null): string {
		if (!p) return '—';
		const segs = p.split('/').filter(Boolean);
		return segs.length > 0 ? (segs[segs.length - 1] ?? p) : p;
	}

	function stringifyDetailValue(value: unknown): string {
		if (typeof value === 'string') return value;
		if (typeof value === 'number' || typeof value === 'boolean') return String(value);
		if (value === null) return '—';
		try {
			return JSON.stringify(value);
		} catch {
			return String(value);
		}
	}

	function handleRestore(): void {
		onRestore?.(action);
	}
</script>

<article class="dac-card" data-augmented-ui="tl-clip br-clip border" data-action-id={action.id}>
	<header class="dac-head">
		<span class="badge-nx badge-{kindInfo.tone}">{kindInfo.label}</span>
		<span class="badge-nx badge-{outcomeTone(action.outcome)}">{action.outcome}</span>
		<span
			class="dac-when font-mono text-micro tabular-nums text-[var(--text-faint)]"
			title={formatClockAbsolute(action.created_at)}
		>
			{formatRelative(action.created_at)}
		</span>
		{#if canRestore}
			<button
				type="button"
				class="dac-restore"
				disabled={restoring}
				onclick={handleRestore}
				aria-label="Restore this lesson from the archive"
			>
				<Undo2 size={12} aria-hidden="true" />
				{restoring ? 'Restoring…' : 'Restore'}
			</button>
		{:else if restored}
			<span class="dac-restored">
				<Undo2 size={12} aria-hidden="true" />
				Restored
			</span>
		{/if}
	</header>

	{#if restoreError}
		<p class="dac-restore-error" role="alert">Couldn't restore: {restoreError}</p>
	{/if}

	<div class="dac-provenance" role="group" aria-label="Provenance">
		{#if action.project_path}
			<a
				class="dac-chip dac-chip--link"
				href={resolve(`projects/${encodeURIComponent(action.project_path)}` as const)}
				title={action.project_path}
			>
				{shortProject(action.project_path)}
			</a>
		{/if}
		{#if action.session_uuid}
			<a
				class="dac-chip dac-chip--link"
				href={resolve(`sessions/${encodeURIComponent(action.session_uuid)}` as const)}
				aria-label="Open session {action.session_uuid}"
			>
				session {shortSessionId(action.session_uuid)}
			</a>
		{/if}
		{#if action.injection_count !== null}
			<span class="dac-chip">
				injected into {action.injection_count}
				{action.injection_count === 1 ? 'request' : 'requests'}
			</span>
		{/if}
		{#if action.decided_by}
			<span class="dac-chip">decided by {action.decided_by.replaceAll('_', ' ')}</span>
		{/if}
	</div>

	{#if action.files_touched && action.files_touched.length > 0}
		<ul class="dac-files" role="list" aria-label="Files touched">
			{#each action.files_touched as file (file)}
				<li class="dac-file">{file}</li>
			{/each}
		</ul>
	{/if}

	{#if !hasCounterpart}
		<p class="dac-text">{action.lesson_text}</p>
	{/if}

	{#if action.error}
		<p class="dac-error" role="alert">{action.error}</p>
	{/if}

	{#if hasCounterpart}
		<div class="dac-compare-meta">
			{#if action.distance !== null}
				<span>distance {action.distance.toFixed(4)}</span>
			{/if}
			{#if action.threshold !== null}
				<span>threshold in effect {action.threshold.toFixed(4)}</span>
			{/if}
			{#if action.embedding_model}
				<span>{action.embedding_model}</span>
			{/if}
		</div>
		<div class="dac-compare-grid">
			<div class="dac-compare-cell">
				<span class="dac-compare-label">This lesson</span>
				<p class="dac-compare-text">{action.lesson_text}</p>
			</div>
			<div class="dac-compare-cell">
				<span class="dac-compare-label"
					>Compared against{action.counterpart_lesson_id
						? ` — lesson #${action.counterpart_lesson_id}`
						: ''}</span
				>
				<p class="dac-compare-text">{action.counterpart_text}</p>
			</div>
		</div>
	{/if}

	{#if action.runners_up.length > 0}
		<div class="dac-runners-up">
			<span class="dac-runners-up__label">Runners-up</span>
			<ul role="list">
				{#each action.runners_up as runnerUp (runnerUp.lesson_id)}
					<li>lesson #{runnerUp.lesson_id} — distance {runnerUp.distance.toFixed(4)}</li>
				{/each}
			</ul>
		</div>
	{/if}

	{#if rankDetailEntries.length > 0}
		<dl class="dac-rank-detail">
			{#each rankDetailEntries as [key, value] (key)}
				<dt>{key.replaceAll('_', ' ')}</dt>
				<dd>{stringifyDetailValue(value)}</dd>
			{/each}
		</dl>
	{/if}
</article>

<style>
	.dac-card {
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

	.dac-head {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.dac-when {
		margin-left: auto;
	}

	.dac-restore {
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
	.dac-restore:hover:not(:disabled) {
		border-color: var(--interactive-active-border);
		color: var(--text-strong);
	}
	.dac-restore:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.dac-restore:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.dac-restored {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		color: var(--status-success-fg);
		font-size: var(--text-label);
	}

	.dac-restore-error {
		margin: 0;
		font-size: var(--text-label);
		color: var(--status-danger-fg);
	}

	.dac-provenance {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.dac-chip {
		display: inline-flex;
		align-items: center;
		border-radius: 4px;
		background: var(--surface-2);
		color: var(--text-2);
		padding: 2px 6px;
		font-size: var(--text-label);
		font-family: var(--font-mono, monospace);
	}
	.dac-chip--link {
		color: var(--interactive-active-fg);
		text-decoration: none;
	}
	.dac-chip--link:hover {
		text-decoration: underline;
	}

	.dac-files {
		display: flex;
		flex-direction: column;
		gap: 2px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.dac-file {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}

	.dac-text {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
	}

	.dac-error {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: var(--text-label);
		color: var(--status-danger-fg);
	}

	.dac-compare-meta {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
		color: var(--text-2);
	}

	.dac-compare-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1px;
		background: var(--border-default);
		border: 1px solid var(--border-default);
		border-radius: 4px;
		overflow: hidden;
	}
	.dac-compare-cell {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px;
		background: var(--surface-card);
		min-width: 0;
	}
	.dac-compare-label {
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-2);
	}
	.dac-compare-text {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
	}

	.dac-runners-up {
		font-size: var(--text-label);
		color: var(--text-2);
	}
	.dac-runners-up__label {
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.dac-runners-up ul {
		margin: 4px 0 0;
		padding: 0 0 0 16px;
	}

	.dac-rank-detail {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 2px 8px;
		margin: 0;
		font-size: var(--text-label);
	}
	.dac-rank-detail dt {
		color: var(--text-2);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.dac-rank-detail dd {
		margin: 0;
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
	}

	@media (max-width: 640px) {
		.dac-compare-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
