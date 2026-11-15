<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLInputAttributes, HTMLTextareaAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';

	type FieldProps = Omit<HTMLInputAttributes, 'class' | 'children' | 'value'> & {
		ref?: HTMLInputElement | HTMLTextAreaElement | null;
		class?: string;
		value?: string | null;
		label?: string | Snippet;
		hint?: string;
		multiline?: boolean;
		rows?: number;
		/**
		 * A refusal to show under the frame. `undefined` draws nothing extra; a string or `null`
		 * always reserves the line, so a message appearing does not move the fields below.
		 */
		error?: string | null;
	};

	let {
		ref = $bindable(null),
		value = $bindable(''),
		class: className,
		label,
		hint,
		multiline = false,
		rows = 3,
		type = 'text',
		error,
		...restProps
	}: FieldProps = $props();

	const uid = $props.id();
	const errorId = $derived(`${uid}-error`);
	const message = $derived(error === undefined || error === null || error === '' ? null : error);
	const a11y = $derived(
		error === undefined
			? {}
			: { 'aria-describedby': errorId, 'aria-invalid': message === null ? undefined : true }
	);
</script>

{#snippet control()}
	{#if label || hint}
		<span class="machine fld__label" data-slot="field-label">
			{#if typeof label === 'string'}{label}{:else if label}{@render label()}{/if}
			{#if hint}<span class="fld__hint">{hint}</span>{/if}
		</span>
	{/if}
	<span class="edge fld__frame" data-multiline={multiline ? '' : undefined}>
		{#if multiline}
			<textarea bind:this={ref} bind:value {rows} {...a11y} {...restProps as HTMLTextareaAttributes}
			></textarea>
		{:else}
			<input bind:this={ref} bind:value {type} {...a11y} {...restProps} />
		{/if}
	</span>
{/snippet}

{#if error === undefined}
	<label class={cn('fld', className)} data-slot="field">
		{@render control()}
	</label>
{:else}
	<div
		class={cn('fld fld--checked', className)}
		data-slot="field"
		data-invalid={message === null ? undefined : ''}
	>
		<label class="fld">
			{@render control()}
		</label>
		<p class="fld__error" id={errorId} role="alert">{message}</p>
	</div>
{/if}

<style>
	.fld {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-inline-size: 0;
	}

	.fld__label {
		font-size: var(--text-xs);
		color: var(--text-2);
	}

	.fld:focus-within .fld__label {
		color: var(--interactive-focus-ring);
	}

	.fld__hint {
		font-family: var(--font-mono);
		text-transform: none;
		letter-spacing: normal;
		color: var(--text-3);
	}

	.fld__frame {
		--edge: var(--edge-sm);
		--edge-pad: 0px;
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-2);
		display: flex;
		align-items: center;
		flex: 0 0 auto;
		block-size: var(--control-h);
	}

	.fld__frame[data-multiline] {
		align-items: stretch;
		block-size: auto;
		min-block-size: var(--control-h);
	}

	.fld__frame:focus-within {
		--edge-line: var(--interactive-focus-ring);
	}

	.fld--checked {
		gap: var(--space-1);
	}

	.fld--checked[data-invalid] .fld__frame {
		--edge-line: var(--status-danger-border);
	}

	.fld__error {
		margin: 0;
		min-block-size: 1.4em;
		font-size: var(--text-xs);
		line-height: 1.4;
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}

	.fld__frame > input,
	.fld__frame > textarea {
		flex: 1 1 0;
		min-inline-size: 0;
		background: transparent;
		border: 0;
		outline: none;
		padding-inline: calc(var(--edge-sm) + var(--space-1));
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--text-1);
	}

	.fld__frame > input {
		block-size: 100%;
	}

	.fld__frame > textarea {
		padding-block: var(--space-2);
		line-height: 1.5;
		resize: vertical;
	}

	.fld__frame > input::placeholder,
	.fld__frame > textarea::placeholder {
		color: var(--text-3);
	}

	.fld__frame > input:disabled,
	.fld__frame > textarea:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}
</style>
