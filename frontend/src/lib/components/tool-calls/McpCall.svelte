<script lang="ts">
	import { Blocks } from '@lucide/svelte';
	import { isRecord, parseMcpName } from './toolRegistry';

	let { name, input }: { name: string; input: unknown } = $props();

	const parsed = $derived(parseMcpName(name));

	function fieldValue(v: unknown): string {
		if (typeof v === 'string') return v;
		try {
			return JSON.stringify(v);
		} catch {
			return String(v);
		}
	}
	const args = $derived(isRecord(input) ? Object.entries(input) : []);
</script>

<div class="mcp-call">
	<div class="mcp-call__head">
		<Blocks size={13} aria-hidden="true" class="mcp-call__icon" />
		{#if parsed !== null}
			<span class="mcp-call__server">{parsed.server}</span>
			<span class="mcp-call__sep" aria-hidden="true">·</span>
			<span class="mcp-call__method">{parsed.method || '(no method)'}</span>
		{:else}
			<span class="mcp-call__method">{name}</span>
		{/if}
	</div>

	{#if args.length > 0}
		<dl class="mcp-call__args">
			{#each args as [key, value] (key)}
				<div class="mcp-call__arg">
					<dt class="mcp-call__arg-k">{key}</dt>
					<dd class="mcp-call__arg-v">{fieldValue(value)}</dd>
				</div>
			{/each}
		</dl>
	{/if}
</div>

<style>
	.mcp-call {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.mcp-call__head {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		min-width: 0;
	}
	.mcp-call :global(.mcp-call__icon) {
		color: var(--data-viz-5-fg);
		flex-shrink: 0;
	}
	.mcp-call__server {
		padding: 1px 8px;
		border: 1px solid var(--data-viz-5-border);
		border-radius: 2px;
		background: var(--data-viz-5-bg);
		color: var(--data-viz-5-fg);
		font-family: var(--font-mono);
		font-size: var(--text-label);
		word-break: break-all;
		overflow-wrap: anywhere;
	}
	.mcp-call__sep {
		color: var(--text-faint);
	}
	.mcp-call__method {
		font-family: var(--font-mono);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-strong);
		word-break: break-all;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.mcp-call__args {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 0;
	}
	.mcp-call__arg {
		display: grid;
		grid-template-columns: minmax(80px, max-content) 1fr;
		gap: 10px;
		align-items: baseline;
	}
	.mcp-call__arg-k {
		font-family: var(--font-mono);
		font-size: var(--text-label);
		color: var(--text-3);
		white-space: nowrap;
	}
	.mcp-call__arg-v {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-1);
		word-break: break-word;
		overflow-wrap: anywhere;
		white-space: pre-wrap;
		min-width: 0;
	}
</style>
