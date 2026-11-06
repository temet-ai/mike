<script lang="ts">
	import { FilePen } from '@lucide/svelte';
	import { isRecord, strField } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	interface Change {
		oldString: string;
		newString: string;
		replaceAll: boolean;
	}

	const filePath = $derived(strField(input, 'file_path') ?? strField(input, 'path'));

	function toChange(rec: Record<string, unknown>): Change {
		return {
			oldString: typeof rec.old_string === 'string' ? rec.old_string : '',
			newString: typeof rec.new_string === 'string' ? rec.new_string : '',
			replaceAll: rec.replace_all === true
		};
	}

	const changes = $derived.by<Change[]>(() => {
		if (!isRecord(input)) return [];
		const edits = input.edits;
		if (Array.isArray(edits)) {
			return edits.filter(isRecord).map(toChange);
		}
		if (typeof input.old_string === 'string' || typeof input.new_string === 'string') {
			return [toChange(input)];
		}
		return [];
	});
</script>

<div class="edit-call">
	{#if filePath !== null}
		<div class="edit-call__path-row">
			<FilePen size={13} aria-hidden="true" class="edit-call__icon" />
			<span class="edit-call__path">{filePath}</span>
			{#if changes.length > 1}
				<span class="edit-call__count">{changes.length} edits</span>
			{/if}
		</div>
	{/if}

	{#if changes.length > 0}
		<ol class="edit-call__changes" aria-label="Edits applied to {filePath ?? 'file'}">
			{#each changes as change, i (i)}
				<li class="edit-call__change">
					{#if change.replaceAll}
						<span class="edit-call__flag">replace all</span>
					{/if}
					<div class="edit-call__diff edit-call__diff--old">
						<span class="edit-call__gutter" aria-hidden="true">−</span>
						<pre class="edit-call__text">{change.oldString}</pre>
					</div>
					<div class="edit-call__diff edit-call__diff--new">
						<span class="edit-call__gutter" aria-hidden="true">+</span>
						<pre class="edit-call__text">{change.newString}</pre>
					</div>
				</li>
			{/each}
		</ol>
	{:else if filePath === null}
		<span class="edit-call__empty">No edit fields in input.</span>
	{/if}
</div>

<style>
	.edit-call {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.edit-call__path-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		min-width: 0;
	}
	.edit-call :global(.edit-call__icon) {
		color: var(--role-tool-fg);
		flex-shrink: 0;
		margin-top: 2px;
	}
	.edit-call__path {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-strong);
		word-break: break-all;
		overflow-wrap: anywhere;
		min-width: 0;
		flex: 1 1 auto;
	}
	.edit-call__count {
		flex-shrink: 0;
		padding: 1px 7px;
		border: 1px solid var(--role-tool-border);
		border-radius: 2px;
		background: var(--role-tool-bg);
		color: var(--role-tool-fg);
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
	}
	.edit-call__changes {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.edit-call__change {
		display: flex;
		flex-direction: column;
		gap: 0;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		overflow: hidden;
	}
	.edit-call__flag {
		align-self: flex-start;
		margin: 6px 6px 0;
		padding: 0 6px;
		border-radius: 2px;
		background: var(--status-muted-bg);
		color: var(--status-muted-fg);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	.edit-call__diff {
		display: flex;
		gap: 8px;
		padding: 6px 10px;
	}
	.edit-call__diff--old {
		background: var(--status-danger-bg);
	}
	.edit-call__diff--new {
		background: var(--status-success-bg);
	}
	.edit-call__gutter {
		flex-shrink: 0;
		font-family: var(--font-mono);
		font-size: 12px;
		font-weight: 600;
		line-height: 1.6;
		user-select: none;
	}
	.edit-call__diff--old .edit-call__gutter {
		color: var(--status-danger-fg);
	}
	.edit-call__diff--new .edit-call__gutter {
		color: var(--status-success-fg);
	}
	.edit-call__text {
		margin: 0;
		min-width: 0;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
		overflow-wrap: anywhere;
	}
	.edit-call__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
