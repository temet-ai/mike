<script lang="ts">
	import { Bot } from '@lucide/svelte';
	import { strField } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	const subagentType = $derived(strField(input, 'subagent_type'));
	const description = $derived(strField(input, 'description'));
	const prompt = $derived(strField(input, 'prompt'));
</script>

<div class="task-call">
	<div class="task-call__head">
		<Bot size={13} aria-hidden="true" class="task-call__icon" />
		{#if subagentType !== null}
			<span class="task-call__agent">{subagentType}</span>
		{/if}
		{#if description !== null}
			<span class="task-call__desc">{description}</span>
		{/if}
	</div>

	{#if prompt !== null}
		<div class="task-call__prompt">
			<span class="task-call__prompt-label">prompt</span>
			<pre class="task-call__prompt-text">{prompt}</pre>
		</div>
	{:else if subagentType === null && description === null}
		<span class="task-call__empty">No task fields in input.</span>
	{/if}
</div>

<style>
	.task-call {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.task-call__head {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
		min-width: 0;
	}
	.task-call :global(.task-call__icon) {
		color: var(--data-teal-fg);
		flex-shrink: 0;
	}
	.task-call__agent {
		padding: 1px 8px;
		border: 1px solid var(--data-teal-border);
		border-radius: 2px;
		background: var(--data-teal-bg);
		color: var(--data-teal-fg);
		font-family: var(--font-mono);
		font-size: var(--text-label);
		white-space: nowrap;
	}
	.task-call__desc {
		font-size: 12px;
		color: var(--text-strong);
		font-weight: 500;
		word-break: break-word;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.task-call__prompt {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.task-call__prompt-label {
		font-size: var(--text-label);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.task-call__prompt-text {
		margin: 0;
		padding: 8px 10px;
		background: var(--surface-code);
		border: 1px solid var(--border-subtle);
		border-left: 2px solid var(--data-teal-fg);
		border-radius: 2px;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
		overflow-wrap: anywhere;
	}
	.task-call__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
