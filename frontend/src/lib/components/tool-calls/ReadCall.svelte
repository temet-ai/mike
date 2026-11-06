<script lang="ts">
	import { FileText } from '@lucide/svelte';
	import { strField, numField } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	const filePath = $derived(strField(input, 'file_path') ?? strField(input, 'path'));
	const offset = $derived(numField(input, 'offset'));
	const limit = $derived(numField(input, 'limit'));

	const rangeLabel = $derived.by(() => {
		if (offset === null && limit === null) return null;
		const start = offset ?? 1;
		if (limit !== null) {
			const end = start + limit - 1;
			return `lines ${start}–${end}`;
		}
		return `from line ${start}`;
	});
</script>

{#if filePath !== null}
	<div class="read-call">
		<div class="read-call__path-row">
			<FileText size={13} aria-hidden="true" class="read-call__icon" />
			<span class="read-call__path">{filePath}</span>
		</div>
		{#if rangeLabel}
			<span class="read-call__range">{rangeLabel}</span>
		{/if}
	</div>
{:else}
	<span class="read-call__empty">No file path in input.</span>
{/if}

<style>
	.read-call {
		display: flex;
		flex-direction: column;
		gap: 5px;
	}
	.read-call__path-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		min-width: 0;
	}
	.read-call :global(.read-call__icon) {
		color: var(--data-teal-fg);
		flex-shrink: 0;
		margin-top: 2px;
	}
	.read-call__path {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-strong);
		word-break: break-all;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.read-call__range {
		align-self: flex-start;
		padding: 1px 7px;
		border: 1px solid var(--data-teal-border);
		border-radius: 2px;
		background: var(--data-teal-bg);
		color: var(--data-teal-fg);
		font-family: var(--font-mono);
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
	}
	.read-call__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
