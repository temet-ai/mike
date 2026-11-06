<script module lang="ts">
	export type SetupTone = 'ok' | 'warn' | 'fail' | 'info';
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
	import CodeBlock from '$lib/components/message-view/CodeBlock.svelte';

	interface Props {
		label: string;
		detail?: string;
		tone?: SetupTone;
		status?: string;
		commands?: string[];
		wide?: boolean;
		children?: Snippet;
	}

	const { label, detail, tone, status, commands = [], wide = false, children }: Props = $props();

	const VARIANT: Record<SetupTone, BadgeVariant> = {
		ok: 'chip-active',
		warn: 'chip-warning',
		fail: 'chip-error',
		info: 'chip-muted'
	};
	const DEFAULT_STATUS: Record<SetupTone, string> = {
		ok: 'Ok',
		warn: 'Warning',
		fail: 'Failed',
		info: ''
	};

	const shownStatus = $derived(tone === undefined ? '' : (status ?? DEFAULT_STATUS[tone]));
</script>

<div class="setup-row" data-slot="setup-row" data-tone={tone}>
	<div class="setup-row-head">
		<span class="setup-row-label">{label}</span>
		{#if tone !== undefined && shownStatus !== ''}
			<Badge variant={VARIANT[tone]}>{shownStatus}</Badge>
		{/if}
	</div>
	{#if detail}
		<p class="setup-row-detail">{detail}</p>
	{/if}
	{#if children}
		<div class="setup-row-controls" data-wide={wide ? '' : undefined}>{@render children()}</div>
	{/if}
	{#each commands as command (command)}
		<CodeBlock lang="terminal" code={command} />
	{/each}
</div>

<style>
	.setup-row {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3) calc(var(--space-4) + var(--space-1));
		border-bottom: var(--hairline) solid var(--border-subtle);
		background: var(--surface-card);
		min-width: 0;
	}
	.setup-row:last-child {
		border-bottom: none;
	}
	.setup-row-head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
	.setup-row-label {
		color: var(--text-strong);
		font-size: var(--text-sm);
		font-weight: var(--weight-display);
		overflow-wrap: anywhere;
	}
	.setup-row-detail {
		margin: 0;
		color: var(--text-2);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		overflow-wrap: anywhere;
	}
	.setup-row-controls {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: var(--space-3);
	}
	.setup-row-controls > :global([data-slot='select']),
	.setup-row-controls > :global([data-slot='field']) {
		flex: 1 1 14rem;
		max-inline-size: 24rem;
	}
	.setup-row-controls[data-wide] > :global(*) {
		flex-basis: 100%;
		max-inline-size: none;
	}
</style>
