<script lang="ts">
	import { X, Plus } from '@lucide/svelte';
	import { untrack } from 'svelte';

	interface Props {
		items: string[];
		maxLen?: number;
		disabled?: boolean;
		onSave?: (items: string[]) => void;
	}

	let { items = [], maxLen, disabled = false, onSave }: Props = $props();

	let localItems = $state<string[]>(untrack(() => [...items]));

	$effect(() => {
		localItems = [...items];
	});

	function commit(): void {
		const nonEmpty = localItems.filter((s) => s.length > 0);
		onSave?.(nonEmpty);
	}

	function handleChange(index: number, event: Event): void {
		const input = event.currentTarget as HTMLInputElement;
		const next = [...localItems];
		next[index] = input.value;
		localItems = next;
	}

	function handleBlur(): void {
		commit();
	}

	function handleKeydown(event: KeyboardEvent): void {
		if (event.key === 'Enter') {
			event.preventDefault();
			(event.currentTarget as HTMLElement).blur();
		}
	}

	function addItem(): void {
		localItems = [...localItems, ''];
		setTimeout(() => {
			const inputs = document.querySelectorAll('.cfg-list-input');
			const last = inputs[inputs.length - 1] as HTMLInputElement | null;
			last?.focus();
		}, 0);
	}

	function removeItem(index: number): void {
		const next = [...localItems];
		next.splice(index, 1);
		localItems = next;
		const nonEmpty = next.filter((s) => s.length > 0);
		onSave?.(nonEmpty);
	}
</script>

<div class="cfg-list" class:cfg-list--disabled={disabled}>
	{#each localItems as item, i (i)}
		<div class="cfg-list-row">
			<input
				type="text"
				class="cfg-list-input"
				value={item}
				maxlength={maxLen}
				{disabled}
				aria-label="Item {i + 1}"
				onchange={(e) => handleChange(i, e)}
				onblur={handleBlur}
				onkeydown={handleKeydown}
			/>
			<button
				type="button"
				class="cfg-list-remove"
				aria-label="Remove item {i + 1}"
				{disabled}
				onclick={() => removeItem(i)}
			>
				<X size={12} aria-hidden="true" />
			</button>
		</div>
	{/each}

	{#if localItems.length === 0}
		<p class="cfg-list-empty">No items. Click Add to add one.</p>
	{/if}

	<span class="cfg-list-add-backing">
		<button
			type="button"
			class="cfg-list-add"
			data-augmented-ui="tr-clip bl-clip border"
			{disabled}
			onclick={addItem}
		>
			<Plus size={11} aria-hidden="true" />
			Add
		</button>
	</span>
</div>

<style>
	.cfg-list {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.cfg-list--disabled {
		opacity: 0.45;
		pointer-events: none;
	}

	.cfg-list-row {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.cfg-list-input {
		flex: 1 1 0;
		min-width: 0;
		padding: 5px 8px;
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-1);
		border-radius: 4px;
		font-family: inherit;
		font-size: 12px;
		transition: border-color 100ms ease-out;
	}

	.cfg-list-input:hover:not(:disabled) {
		border-color: var(--text-3);
	}

	.cfg-list-input:focus {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-color: var(--interactive-focus-ring);
	}

	.cfg-list-input:disabled {
		cursor: not-allowed;
	}

	.cfg-list-remove {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border: 1px solid var(--border-default);
		background: transparent;
		color: var(--text-3);
		border-radius: 3px;
		cursor: pointer;
		flex-shrink: 0;
		transition:
			color 100ms ease-out,
			border-color 100ms ease-out;
	}

	.cfg-list-remove:hover:not(:disabled) {
		color: var(--status-danger-fg);
		border-color: var(--status-danger-border);
	}

	.cfg-list-remove:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.cfg-list-remove:disabled {
		cursor: not-allowed;
	}

	.cfg-list-empty {
		margin: 0;
		font-family: inherit;
		font-size: 11px;
		color: var(--text-3);
		font-style: italic;
	}

	.cfg-list-add-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--surface-card);
	}
	.cfg-list-add {
		--edge: 5px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 5px);
		background: var(--surface-2);
		color: var(--text-2);
		font-family: inherit;
		font-size: 11px;
		cursor: pointer;
		transition:
			color 100ms ease-out,
			background 100ms ease-out;
	}

	.cfg-list-add:hover:not(:disabled) {
		color: var(--text-1);
		--aug-border-bg: var(--interactive-active-border);
	}

	.cfg-list-add:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.cfg-list-add:disabled {
		cursor: not-allowed;
	}
</style>
