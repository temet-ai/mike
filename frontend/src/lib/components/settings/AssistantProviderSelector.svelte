<script lang="ts">
	import { patchSetting } from '$lib/api/settings';

	interface Props {
		provider: 'anthropic' | 'codex';
		onChanged?: (provider: 'anthropic' | 'codex') => void;
	}

	let { provider, onChanged }: Props = $props();
	let saving = $state(false);
	let error = $state<string | null>(null);

	async function select(next: 'anthropic' | 'codex'): Promise<void> {
		if (saving || next === provider) return;
		saving = true;
		error = null;
		try {
			await patchSetting('assistant.provider', next);
			onChanged?.(next);
		} catch (reason) {
			error = reason instanceof Error ? reason.message : String(reason);
		} finally {
			saving = false;
		}
	}
</script>

<section class="aps" aria-labelledby="aps-title">
	<div>
		<h2 id="aps-title">Prompt assistant provider</h2>
		<p>Choose which signed-in integration handles new assistant messages.</p>
	</div>
	<div class="aps-options" aria-busy={saving}>
		<button
			class:active={provider === 'anthropic'}
			disabled={saving}
			onclick={() => select('anthropic')}
		>
			<span>Anthropic</span><small>Claude Code or API key</small>
		</button>
		<button class:active={provider === 'codex'} disabled={saving} onclick={() => select('codex')}>
			<span>Codex</span><small>Existing ChatGPT login</small>
		</button>
	</div>
	{#if error}<p class="aps-error" role="alert">Could not save provider: {error}</p>{/if}
</section>

<style>
	.aps {
		display: grid;
		grid-template-columns: minmax(220px, 1fr) minmax(320px, 1.4fr);
		gap: 24px;
		padding: 20px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-card);
	}
	h2 {
		margin: 0;
		color: var(--text-strong);
		font: 700 14px var(--font-display);
	}
	p {
		margin: 6px 0 0;
		color: var(--text-2);
		font-size: 12px;
		line-height: 1.5;
	}
	.aps-options {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
	}
	button {
		display: flex;
		flex-direction: column;
		gap: 4px;
		text-align: left;
		padding: 12px 14px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-bg);
		color: var(--text-1);
		cursor: pointer;
	}
	button.active {
		border-color: var(--interactive-active-border);
		background: var(--surface-2);
	}
	button:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	button:disabled {
		cursor: wait;
		opacity: 0.7;
	}
	button span {
		font-weight: 700;
	}
	button small {
		color: var(--text-3);
	}
	.aps-error {
		grid-column: 1 / -1;
		color: var(--status-danger-fg);
	}
	@media (max-width: 680px) {
		.aps {
			grid-template-columns: 1fr;
		}
	}
</style>
