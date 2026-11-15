<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLSelectAttributes } from 'svelte/elements';
	import { ChevronDown } from '@lucide/svelte';
	import { cn } from '$lib/utils';

	export interface SelectOption {
		value: string;
		label?: string;
		disabled?: boolean;
	}

	type SelectProps = Omit<HTMLSelectAttributes, 'class' | 'children' | 'value'> & {
		ref?: HTMLSelectElement | null;
		class?: string;
		value?: string;
		options: SelectOption[];
		label?: string | Snippet;
		hint?: string;
		/** A refusal under the frame; `undefined` draws nothing extra, `null` reserves the line. */
		error?: string | null;
	};

	let {
		ref = $bindable(null),
		value = $bindable(''),
		class: className,
		options,
		label,
		hint,
		error,
		...restProps
	}: SelectProps = $props();

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
		<span class="machine sel__label" data-slot="select-label">
			{#if typeof label === 'string'}{label}{:else if label}{@render label()}{/if}
			{#if hint}<span class="sel__hint">{hint}</span>{/if}
		</span>
	{/if}
	<span class="edge sel__frame">
		<select bind:this={ref} bind:value {...a11y} {...restProps}>
			{#each options as option (option.value)}
				<option value={option.value} disabled={option.disabled}>
					{option.label ?? option.value}
				</option>
			{/each}
		</select>
		<ChevronDown class="sel__caret" size={12} aria-hidden="true" />
	</span>
{/snippet}

{#if error === undefined}
	<label class={cn('sel', className)} data-slot="select">
		{@render control()}
	</label>
{:else}
	<div
		class={cn('sel sel--checked', className)}
		data-slot="select"
		data-invalid={message === null ? undefined : ''}
	>
		<label class="sel">
			{@render control()}
		</label>
		<p class="sel__error" id={errorId} role="alert">{message}</p>
	</div>
{/if}

<style>
	.sel {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-inline-size: 0;
	}

	.sel__label {
		color: var(--text-2);
		font-size: var(--text-xs);
	}

	.sel:focus-within .sel__label {
		color: var(--interactive-focus-ring);
	}

	.sel__hint {
		color: var(--text-3);
		font-family: var(--font-mono);
		letter-spacing: normal;
		text-transform: none;
	}

	.sel__frame {
		--edge: var(--edge-sm);
		--edge-pad: 0px;
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-2);
		display: flex;
		align-items: center;
		flex: 0 0 auto;
		block-size: var(--control-h);
	}

	.sel__frame:focus-within {
		--edge-line: var(--interactive-focus-ring);
	}

	.sel--checked {
		gap: var(--space-1);
	}

	.sel--checked[data-invalid] .sel__frame {
		--edge-line: var(--status-danger-border);
	}

	.sel__error {
		margin: 0;
		min-block-size: 1.4em;
		font-size: var(--text-xs);
		line-height: 1.4;
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}

	.sel__frame > select {
		flex: 1 1 0;
		min-inline-size: 0;
		block-size: 100%;
		padding-inline: calc(var(--edge-sm) + var(--space-1)) var(--space-6);
		border: 0;
		outline: none;
		appearance: none;
		background: transparent;
		color: var(--text-1);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		cursor: pointer;
	}

	.sel__frame > select:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}

	.sel__frame > select > option {
		background: var(--surface-2);
		color: var(--text-1);
	}

	.sel__frame > :global(.sel__caret) {
		position: absolute;
		inset-inline-end: var(--space-3);
		color: var(--text-3);
		pointer-events: none;
	}
</style>
