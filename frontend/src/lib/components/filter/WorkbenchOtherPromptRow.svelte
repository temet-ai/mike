<script lang="ts">
	import { Switch } from '$lib/components/ui/switch';
	import { getContext } from 'svelte';
	import { ArrowUpRight } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { FILTER_RULES_STORE_KEY, type FilterRulesStore } from '$lib/stores/filterRules.svelte';
	import type { FilterRuleDto } from '$lib/types';
	import { formatRelative } from '$lib/time';
	import { deriveRuleTitle } from './rule-matching';

	interface Props {
		rule: FilterRuleDto;
		/** The request id to jump to for this rule's applicable prompt, or null in the
		 *  "matches no recent prompt" group, where there is nowhere to jump. */
		latestRequestId: string | null;
	}

	let { rule, latestRequestId }: Props = $props();

	const store = getContext<FilterRulesStore | undefined>(FILTER_RULES_STORE_KEY);

	let busy = $state(false);
	let confirmingDelete = $state(false);

	const title = $derived(deriveRuleTitle(rule));
	const jumpHref = $derived(
		latestRequestId !== null
			? resolve(`/filter/workbench?from=request:${latestRequestId}&rule=${rule.id}`)
			: null
	);
	const lastFiredText = $derived(
		rule.last_fired_at !== null ? `last fired ${formatRelative(rule.last_fired_at)}` : 'never fired'
	);

	async function onToggleEnabled(): Promise<void> {
		if (!store || busy) return;
		busy = true;
		try {
			await store.update(rule.id, { enabled: !rule.enabled });
		} finally {
			busy = false;
		}
	}

	function requestDelete(): void {
		confirmingDelete = true;
	}

	function cancelDelete(): void {
		confirmingDelete = false;
	}

	async function confirmDelete(): Promise<void> {
		if (!store || busy) return;
		busy = true;
		try {
			await store.remove(rule.id);
		} finally {
			busy = false;
			confirmingDelete = false;
		}
	}
</script>

<div
	class="wb-other-row"
	class:wb-other-row--disabled={!rule.enabled}
	data-slot="workbench-other-prompt-row"
	data-rule-id={rule.id}
>
	<div class="wb-other-row__identity">
		<span class="wb-other-row__title">{title}</span>
		<span class="wb-other-row__meta">
			{latestRequestId !== null ? lastFiredText : `matches no recent prompt · ${lastFiredText}`}
		</span>
	</div>

	{#if jumpHref !== null}
		<a class="wb-other-row__jump" href={jumpHref} aria-label={`Open in workbench: ${title}`}>
			<ArrowUpRight size={11} aria-hidden="true" />
		</a>
	{/if}

	{#if confirmingDelete}
		<div class="wb-other-row__confirm" role="group" aria-label={`Confirm delete: ${title}`}>
			<span class="wb-other-row__confirm-label">Delete?</span>
			<button
				type="button"
				class="wb-other-row__confirm-btn wb-other-row__confirm-btn--yes"
				onclick={confirmDelete}
				disabled={busy}
			>
				Yes
			</button>
			<button
				type="button"
				class="wb-other-row__confirm-btn"
				onclick={cancelDelete}
				disabled={busy}
			>
				Cancel
			</button>
		</div>
	{:else}
		<button
			type="button"
			class="wb-other-row__delete"
			onclick={requestDelete}
			disabled={busy}
			aria-label={`Delete edit: ${title}`}
		>
			Delete
		</button>
	{/if}

	<Switch
		bind:checked={() => rule.enabled, () => onToggleEnabled()}
		aria-label={rule.enabled ? `Turn off: ${title}` : `Turn on: ${title}`}
		disabled={busy}
	/>
</div>

<style>
	.wb-other-row {
		display: flex;
		align-items: center;
		gap: 6px;
		min-height: 30px;
		padding: 2px 8px 2px 10px;
		transition: opacity 120ms ease;
	}

	.wb-other-row--disabled {
		opacity: 0.55;
	}

	.wb-other-row__identity {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.wb-other-row__title {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		overflow-wrap: break-word;
	}

	.wb-other-row__meta {
		font-size: 10px;
		color: var(--text-3);
	}

	.wb-other-row__jump {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		color: var(--funk-cyan);
		border: 1px solid var(--funk-cyan-border);
		border-radius: 3px;
		text-decoration: none;
	}
	.wb-other-row__jump:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-other-row__delete {
		flex-shrink: 0;
		padding: 2px 7px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-3);
		background: transparent;
		border: 1px solid transparent;
		cursor: pointer;
	}
	.wb-other-row__delete:hover {
		color: var(--status-danger-fg);
		border-color: var(--status-danger-fg);
	}
	.wb-other-row__delete:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-other-row__confirm {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.wb-other-row__confirm-label {
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--status-danger-fg);
	}
	.wb-other-row__confirm-btn {
		height: 20px;
		padding: 0 7px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		background: transparent;
		border: 1px solid var(--border-default);
		color: var(--text-2);
		cursor: pointer;
	}
	.wb-other-row__confirm-btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.wb-other-row__confirm-btn--yes {
		color: var(--status-danger-fg);
		border-color: var(--status-danger-fg);
	}
	.wb-other-row__confirm-btn--yes:hover {
		background: var(--status-danger-fg);
		color: var(--surface-1);
	}
</style>
