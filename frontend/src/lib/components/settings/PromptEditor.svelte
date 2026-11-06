<script lang="ts">
	import { untrack } from 'svelte';

	interface Props {
		value: string;
		defaultValue: string;
		maxLen?: number;
		loading?: boolean;
		disabled?: boolean;
		label: string;
		onSave?: (value: string) => void;
	}

	let {
		value,
		defaultValue,
		maxLen,
		loading = false,
		disabled = false,
		label,
		onSave
	}: Props = $props();

	const uid = $props.id();
	const noticeId = `${uid}-blank-notice`;

	let draft = $state<string>(untrack(() => value));
	let textareaEl: HTMLTextAreaElement | undefined = $state();

	let showBlankNotice = $state(false);

	$effect(() => {
		draft = value;
	});

	$effect(() => {
		if (showBlankNotice && draft.trim() !== '') showBlankNotice = false;
	});

	const dirty = $derived(draft !== value);
	const isDefault = $derived(draft.trim() === defaultValue.trim());
	const isBlank = $derived(value.trim() === '');
	const busy = $derived(loading || disabled);

	function save(): void {
		if (busy || !dirty) return;
		if (draft.trim() === '') {
			showBlankNotice = true;
			return;
		}
		onSave?.(draft);
	}

	function resetToDefault(): void {
		if (busy) return;
		showBlankNotice = false;
		draft = defaultValue;
		onSave?.(defaultValue);
	}

	function revertFromNotice(): void {
		showBlankNotice = false;
		draft = defaultValue;
		onSave?.(defaultValue);
	}

	function keepEditing(): void {
		showBlankNotice = false;
		textareaEl?.focus();
	}

	function handleKeydown(event: KeyboardEvent): void {
		if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
			event.preventDefault();
			save();
		}
	}
</script>

<div class="pe" class:pe--disabled={disabled}>
	<textarea
		bind:this={textareaEl}
		class="pe-text"
		bind:value={draft}
		maxlength={maxLen}
		rows="14"
		spellcheck="false"
		disabled={busy}
		aria-label={label}
		aria-describedby={showBlankNotice ? noticeId : undefined}
		onkeydown={handleKeydown}></textarea>

	{#if showBlankNotice}
		<div class="pe-notice" id={noticeId} role="alert">
			<p class="pe-notice-msg">A prompt cannot be blank. Revert to the built-in prompt?</p>
			<div class="pe-notice-actions">
				<span class="pe-btn-backing pe-btn-backing--notice">
					<button type="button" class="pe-btn pe-btn--save" onclick={revertFromNotice}>
						Revert to default
					</button>
				</span>
				<span class="pe-btn-backing pe-btn-backing--notice">
					<button type="button" class="pe-btn" onclick={keepEditing}> Keep editing </button>
				</span>
			</div>
		</div>
	{/if}

	<div class="pe-actions">
		<span class="pe-btn-backing">
			<button type="button" class="pe-btn pe-btn--save" disabled={busy || !dirty} onclick={save}>
				{loading ? 'Saving' : 'Save'}
			</button>
		</span>
		<span class="pe-btn-backing">
			<button type="button" class="pe-btn" disabled={busy || isDefault} onclick={resetToDefault}>
				Reset to default
			</button>
		</span>

		<span class="pe-status" aria-live="polite">
			{#if dirty}
				Unsaved changes. Save applies them.
			{:else if isBlank}
				No prompt is saved. Save one or reset to the default.
			{:else if isDefault}
				Matches the built-in prompt.
			{:else}
				Saved. This prompt replaces the built-in one.
			{/if}
		</span>

		{#if maxLen !== undefined}
			<span class="pe-count">{draft.length} / {maxLen}</span>
		{/if}
	</div>
</div>

<style>
	.pe {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.pe--disabled {
		opacity: 0.45;
	}

	.pe-text {
		width: 100%;
		min-height: 220px;
		resize: vertical;
		padding: 10px 12px;
		border: 1px solid var(--border-default);
		border-radius: 4px;
		background: var(--surface-bg);
		color: var(--text-1);
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 12px;
		line-height: 1.6;
		white-space: pre-wrap;
		transition: border-color 100ms ease-out;
	}

	.pe-text:hover:not(:disabled) {
		border-color: var(--text-3);
	}

	.pe-text:focus {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-color: var(--interactive-focus-ring);
	}

	.pe-text:disabled {
		cursor: not-allowed;
	}

	.pe-notice {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 10px 12px;
		border: 1px solid var(--status-danger-border);
		border-left: 2px solid var(--status-danger-border);
		border-radius: 6px;
		background: var(--status-danger-bg);
	}

	.pe-notice-msg {
		margin: 0;
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
	}

	.pe-notice-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.pe-btn-backing--notice {
		background: var(--status-danger-bg);
	}

	.pe-actions {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
	}

	.pe-btn-backing {
		display: inline-flex;
		background: var(--surface-card);
	}

	.pe-btn {
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
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 7px);
		background: var(--surface-2);
		color: var(--text-2);
		font-family: inherit;
		font-size: 11px;
		cursor: pointer;
		transition:
			color 100ms ease-out,
			background 100ms ease-out;
	}

	.pe-btn:hover:not(:disabled) {
		color: var(--text-1);
		--aug-border-bg: var(--interactive-active-border);
	}

	.pe-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.pe-btn:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}

	.pe-btn--save:not(:disabled) {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}

	.pe-status {
		font-size: 11px;
		color: var(--text-3);
		line-height: 1.5;
	}

	.pe-count {
		margin-left: auto;
		font-size: 11px;
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
</style>
