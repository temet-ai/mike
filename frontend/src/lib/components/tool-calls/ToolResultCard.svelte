<script lang="ts">
	import { untrack } from 'svelte';
	import { FileOutput } from '@lucide/svelte';
	import { classify, KIND_PRESENTATION } from './toolRegistry';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props {
		toolName?: string;
		content: string;
		isError?: boolean;
		rawOpen?: boolean;
	}

	let { toolName, content, isError = false, rawOpen = false }: Props = $props();

	const kind = $derived(toolName ? classify(toolName) : 'generic');
	const presentation = $derived(KIND_PRESENTATION[kind]);

	const accentVar = $derived(isError ? '--status-danger-fg' : presentation.accentVar);

	const headerSummary = $derived.by(() => {
		if (!content || content.trim().length === 0) return '(no output)';
		const firstLine = content.split('\n').find((l) => l.trim().length > 0) ?? '';
		return firstLine;
	});

	let expanded = $state(untrack(() => rawOpen));
	function toggle(): void {
		expanded = !expanded;
	}
</script>

<article
	class="tool-card tool-result-card"
	data-augmented-ui="tr-clip br-clip border"
	style="--accent: var({accentVar});"
>
	<header class="tool-card__header">
		<span class="tool-card__icon" aria-hidden="true">
			{#if isError}
				<presentation.icon size={14} />
			{:else}
				<FileOutput size={14} />
			{/if}
		</span>
		<span class="tool-result-card__label" class:tool-result-card__label--error={isError}>
			{isError ? 'Error result' : 'Result'}
		</span>
		{#if toolName}
			<span class="tool-card__name">{toolName}</span>
		{/if}
		{#if isError}
			<span class="tool-result-card__error-badge">error</span>
		{/if}
	</header>

	{#if !expanded}
		<div class="tool-result-card__headline">
			<span class="tool-result-card__headline-text">{headerSummary}</span>
		</div>
	{/if}

	<div class="tool-card__raw">
		<button type="button" class="tool-card__raw-toggle" aria-expanded={expanded} onclick={toggle}>
			<DisclosureMarker open={expanded} size={12} />
			<FileOutput size={12} aria-hidden="true" class="raw-braces" />
			<span>{expanded ? 'Hide output' : 'Show output'}</span>
		</button>
		{#if expanded}
			<div class="tool-card__raw-body" aria-label="Full tool result output">
				<pre class="tool-card__raw-json">{content}</pre>
			</div>
		{/if}
	</div>
</article>

<style>
	.tool-card {
		--edge: 8px;
		--aug-tr: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(135deg, var(--accent) 0%, var(--border-default) 60%);
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 10px calc(var(--edge) + 4px) 10px 12px;
		background: var(--surface-card);
		border: 1px solid var(--border-subtle);
		border-left: 3px solid var(--accent);
	}
	.tool-card__header {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}
	.tool-card__icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		color: var(--accent);
		flex-shrink: 0;
	}
	.tool-card__name {
		font-family: var(--font-mono);
		font-size: 13px;
		font-weight: 600;
		color: var(--accent);
		word-break: break-all;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.tool-result-card__label {
		font-family: var(--font-mono);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-3);
		flex-shrink: 0;
	}
	.tool-result-card__label--error {
		color: var(--status-danger-fg);
	}
	.tool-result-card__error-badge {
		flex-shrink: 0;
		padding: 1px 7px;
		border: 1px solid var(--status-danger-border);
		border-radius: 2px;
		background: var(--status-danger-bg);
		color: var(--status-danger-fg);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	.tool-result-card__headline {
		padding: 0 2px;
		min-width: 0;
	}
	.tool-result-card__headline-text {
		font-family: var(--font-mono);
		font-size: var(--text-label);
		color: var(--text-2);
		word-break: break-all;
		overflow-wrap: anywhere;
		white-space: pre-wrap;
	}
	.tool-card__raw {
		display: flex;
		flex-direction: column;
		gap: 6px;
		border-top: 1px solid var(--border-subtle);
		padding-top: 8px;
	}
	.tool-card__raw-toggle {
		display: inline-flex;
		align-items: center;
		align-self: flex-start;
		gap: 6px;
		padding: 0;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-3);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.tool-card__raw-toggle:hover {
		color: var(--text-strong);
	}
	.tool-card__raw-toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	.tool-card__raw-toggle :global(.raw-braces) {
		color: var(--text-faint);
	}
	.tool-card__raw-body {
		background: var(--surface-code);
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		max-height: 360px;
		overflow: auto;
	}
	.tool-card__raw-json {
		margin: 0;
		padding: 10px;
		font-family: var(--font-mono);
		font-size: var(--text-label);
		line-height: 1.6;
		color: var(--text-1);
		white-space: pre;
		word-break: normal;
	}
</style>
