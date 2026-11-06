<script lang="ts">
	import { Download } from '@lucide/svelte';
	import { strField } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	const url = $derived(strField(input, 'url'));
	const prompt = $derived(strField(input, 'prompt') ?? strField(input, 'query'));

	const href = $derived.by(() => {
		if (url === null) return null;
		try {
			const u = new URL(url);
			return u.protocol === 'http:' || u.protocol === 'https:' ? url : null;
		} catch {
			return null;
		}
	});
</script>

<div class="fetch-call">
	{#if url !== null}
		<div class="fetch-call__url-row">
			<Download size={13} aria-hidden="true" class="fetch-call__icon" />
			{#if href !== null}
				<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- external URL from the model's tool call (validated http(s):// above), never an internal route -->
				<a {href} class="fetch-call__url" target="_blank" rel="noopener noreferrer">{url}</a>
			{:else}
				<span class="fetch-call__url fetch-call__url--plain">{url}</span>
			{/if}
		</div>
	{/if}

	{#if prompt}
		<div class="fetch-call__prompt">
			<span class="fetch-call__prompt-label">prompt</span>
			<p class="fetch-call__prompt-text">{prompt}</p>
		</div>
	{:else if url === null}
		<span class="fetch-call__empty">No URL in input.</span>
	{/if}
</div>

<style>
	.fetch-call {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.fetch-call__url-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		min-width: 0;
	}
	.fetch-call :global(.fetch-call__icon) {
		color: var(--agent-4-fg);
		flex-shrink: 0;
		margin-top: 2px;
	}
	.fetch-call__url {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--agent-4-fg);
		word-break: break-all;
		overflow-wrap: anywhere;
		min-width: 0;
		text-decoration: underline;
		text-decoration-color: var(--agent-4-border);
		text-underline-offset: 2px;
	}
	.fetch-call__url:hover {
		text-decoration-color: var(--agent-4-fg);
	}
	.fetch-call__url:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	.fetch-call__url--plain {
		color: var(--text-strong);
		text-decoration: none;
	}
	.fetch-call__prompt {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.fetch-call__prompt-label {
		font-size: var(--text-label);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.fetch-call__prompt-text {
		margin: 0;
		font-size: 12px;
		color: var(--text-1);
		line-height: 1.55;
		word-break: break-word;
		overflow-wrap: anywhere;
	}
	.fetch-call__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
