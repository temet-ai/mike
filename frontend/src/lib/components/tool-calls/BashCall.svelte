<script lang="ts">
	import { strField } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	const command = $derived(strField(input, 'command'));
	const description = $derived(strField(input, 'description'));
</script>

{#if command !== null}
	<div class="bash-call">
		<pre class="bash-call__line"><span class="bash-call__prompt" aria-hidden="true">$</span><code
				class="bash-call__cmd">{command}</code
			></pre>
		{#if description}
			<p class="bash-call__desc">{description}</p>
		{/if}
	</div>
{:else}
	<span class="bash-call__empty">No command in input.</span>
{/if}

<style>
	.bash-call {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.bash-call__line {
		display: flex;
		gap: 8px;
		margin: 0;
		padding: 8px 10px;
		background: var(--surface-code);
		border: 1px solid var(--status-info-border);
		border-left: 2px solid var(--status-info-fg);
		border-radius: 2px;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.6;
		white-space: pre-wrap;
		word-break: break-word;
		overflow-wrap: anywhere;
	}
	.bash-call__prompt {
		flex-shrink: 0;
		color: var(--status-info-fg);
		font-weight: 600;
		user-select: none;
	}
	.bash-call__cmd {
		color: var(--text-strong);
		min-width: 0;
	}
	.bash-call__desc {
		margin: 0;
		font-size: var(--text-label);
		color: var(--text-2);
		font-style: italic;
		word-break: break-word;
		overflow-wrap: anywhere;
	}
	.bash-call__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
