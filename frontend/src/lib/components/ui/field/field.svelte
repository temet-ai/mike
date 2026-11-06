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
		...restProps
	}: FieldProps = $props();
</script>

<label class={cn('fld', className)} data-slot="field">
	{#if label || hint}
		<span class="machine fld__label" data-slot="field-label">
			{#if typeof label === 'string'}{label}{:else if label}{@render label()}{/if}
			{#if hint}<span class="fld__hint">{hint}</span>{/if}
		</span>
	{/if}
	<span class="edge fld__frame" data-multiline={multiline ? '' : undefined}>
		{#if multiline}
			<textarea bind:this={ref} bind:value {rows} {...restProps as HTMLTextareaAttributes}
			></textarea>
		{:else}
			<input bind:this={ref} bind:value {type} {...restProps} />
		{/if}
	</span>
</label>

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
