<script lang="ts">
	import { AlertCircle, ArrowRight, Wrench } from '@lucide/svelte';
	import ClassChip from './ClassChip.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import type { PromptCatalogueRowViewModel } from '$lib/prompts/catalogueRow';
	import type { PromptModelBreakdown } from '$lib/types';

	interface Props {
		row: PromptCatalogueRowViewModel;
		modelBreakdown?: PromptModelBreakdown[];
	}

	let { row, modelBreakdown }: Props = $props();
</script>

<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- row.href is a pre-built internal pathname from $lib/prompts/catalogueRow.ts's PromptCatalogueRowViewModel (typed `string`, outside this sweep's file set); narrowing it to a resolve()-compatible literal type belongs to that file. -->
<a
	href={row.href}
	class="catalogue-row group flex flex-wrap items-center gap-3 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-3 py-2.5 outline-none transition-colors hover:border-[var(--interactive-active-border)] hover:bg-[var(--surface-2)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-active-border)]"
	data-slot="prompt-catalogue-row"
	data-state={row.state}
	aria-label={`${row.actionLabel} — ${row.displayName}`}
>
	{#if row.isHumanizedReason}
		<AlertCircle class="size-3.5 shrink-0 text-[var(--status-warning-fg)]" aria-hidden="true" />
		<span class="text-label text-[var(--text-2)]">{row.displayName}</span>
	{:else}
		<ClassChip label={row.displayName} />
	{/if}

	<span class="badge-nx {row.badgeClass}">
		<span class="badge-nx-dot"></span>
		{row.badgeText}
	</span>

	{#if modelBreakdown && modelBreakdown.length > 0}
		<span class="model-identity" data-slot="prompt-catalogue-row-models">
			{#each modelBreakdown as m, i (m.model ?? `unknown-${i}`)}
				<span class="model-chip">
					{m.model ?? '(unknown model)'}{#if modelBreakdown.length > 1}
						&nbsp;· {m.count.toLocaleString('en-US')}{/if}
				</span>
			{/each}
		</span>
	{/if}

	{#if row.firingRuleCount > 0}
		<Tooltip
			text={`${row.firingRuleCount} filter rule${row.firingRuleCount > 1 ? 's' : ''} rewrote this shape`}
		>
			<Wrench class="size-3.5 shrink-0 text-[var(--text-3)]" aria-hidden="true" />
		</Tooltip>
	{/if}

	<span class="ml-auto font-mono text-xs text-[var(--text-1)]">
		{#if row.observationCount === 0}
			<Tooltip text="No traffic in the selected window">0 obs</Tooltip>
		{:else if row.observationCount === null}
			—
		{:else}
			{row.observationCount.toLocaleString('en-US')}{#if row.lastSeenLabel}
				· {row.lastSeenLabel}{/if}
		{/if}
	</span>

	<span
		class="text-label text-[var(--text-3)] transition-colors group-hover:text-[var(--interactive-active-fg)]"
	>
		{row.actionLabel}
	</span>

	<ArrowRight
		class="size-3.5 shrink-0 text-[var(--text-3)] transition-colors group-hover:text-[var(--interactive-active-fg)]"
		aria-hidden="true"
	/>
</a>

<style>
	.catalogue-row {
		text-decoration: none;
	}

	.model-identity {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 4px;
	}
	.model-chip {
		font-family: 'JetBrains Mono Variable', ui-monospace, monospace;
		font-size: var(--text-label);
		color: var(--text-3);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 3px;
		padding: 1px 5px;
	}
</style>
