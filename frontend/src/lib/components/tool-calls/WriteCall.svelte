<script lang="ts">
	import { FilePlus } from '@lucide/svelte';
	import { strField } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	const filePath = $derived(strField(input, 'file_path') ?? strField(input, 'path'));
	const content = $derived(strField(input, 'content'));
</script>

<div class="write-call">
	{#if filePath !== null}
		<div class="write-call__path-row">
			<FilePlus size={13} aria-hidden="true" class="write-call__icon" />
			<span class="write-call__path">{filePath}</span>
			<span class="write-call__new">new file</span>
		</div>
	{/if}

	{#if content !== null}
		<pre class="write-call__content">{content}</pre>
	{:else if filePath === null}
		<span class="write-call__empty">No write fields in input.</span>
	{/if}
</div>

<style>
	.write-call {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.write-call__path-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		min-width: 0;
	}
	.write-call :global(.write-call__icon) {
		color: var(--status-success-fg);
		flex-shrink: 0;
		margin-top: 2px;
	}
	.write-call__path {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-strong);
		word-break: break-all;
		overflow-wrap: anywhere;
		min-width: 0;
		flex: 1 1 auto;
	}
	.write-call__new {
		flex-shrink: 0;
		padding: 1px 7px;
		border: 1px solid var(--status-success-border);
		border-radius: 2px;
		background: var(--status-success-bg);
		color: var(--status-success-fg);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	.write-call__content {
		margin: 0;
		padding: 8px 10px;
		background: var(--surface-code);
		border: 1px solid var(--border-subtle);
		border-left: 2px solid var(--status-success-fg);
		border-radius: 2px;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
		overflow-wrap: anywhere;
	}
	.write-call__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
