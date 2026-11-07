<script lang="ts">
	import type { RequestDetail } from '$lib/types';
	import { resolve } from '$app/paths';
	import { formatRelative } from '$lib/time';
	import { shortSessionId } from '$lib/format';
	import { subagentBadgeLabel } from '$lib/request-utils';
	import StatusPill from '../shared/StatusPill.svelte';
	import { ArrowLeft } from '@lucide/svelte';

	let {
		detail,
		now,
		sessionId = null
	}: { detail: RequestDetail; now: number; sessionId?: string | null } = $props();

	const personaLabel = $derived(subagentBadgeLabel(detail));

	const metaCells = $derived<Array<[string, string, string | undefined]>>([
		['Model', detail.model ?? '—', undefined],
		['Created', formatRelative(detail.created_at, now), detail.created_at],
		[
			'Completed',
			detail.completed_at ? formatRelative(detail.completed_at, now) : '—',
			detail.completed_at ?? ''
		]
	]);

	const backHref = $derived(
		sessionId ? (`sessions/${encodeURIComponent(sessionId)}` as const) : null
	);
	const backLabel = $derived(sessionId ? `session ${shortSessionId(sessionId)}` : '');
</script>

<header
	class="detail-header flex flex-wrap items-center gap-x-3 gap-y-1 bg-[var(--surface-card)] text-sm"
	data-augmented-ui="tl-clip br-clip border"
	style="--aug-border-bg: linear-gradient(135deg, var(--status-info-fg) 0%, var(--status-info-border) 35%, var(--border-default) 70%);"
>
	{#if backHref}
		<a
			href={resolve(backHref)}
			class="inline-flex shrink-0 items-center gap-1 rounded-[2px] text-xs text-[var(--text-3)] hover:text-[var(--text-1)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-bg)]"
		>
			<ArrowLeft size={12} aria-hidden="true" />
			{backLabel}</a
		>
		<span class="text-[var(--text-faint)]" aria-hidden="true">&#x2502;</span>
	{/if}
	{#if detail.request_number != null}
		<span
			class="shrink-0 font-mono text-sm font-semibold tabular-nums text-[var(--text-strong)]"
			title="Global id: #{detail.id}"
			>#{detail.request_number}<span class="text-xs font-normal text-[var(--text-3)]"
				>&nbsp;·&nbsp;{detail.id}</span
			></span
		>
	{:else}
		<span class="shrink-0 font-mono text-sm font-semibold text-[var(--text-strong)]"
			>#{detail.id}</span
		>
	{/if}
	<span
		class="shrink-0 rounded-[2px] bg-[var(--surface-2)] px-1.5 py-0.5 font-mono text-label text-[var(--text-2)]"
		>{detail.method}</span
	>
	<span class="min-w-0 break-all font-mono text-xs text-[var(--text-2)]" title={detail.path}
		>{detail.path}</span
	>
	{#if personaLabel}
		<span
			class="min-w-0 rounded-[2px] bg-[var(--surface-2)] px-1.5 py-0.5 font-mono text-label break-words text-[var(--text-2)]"
			title={personaLabel}>{personaLabel}</span
		>
	{/if}
	<StatusPill status={detail.status} />
	<div
		class="ml-auto flex shrink-0 flex-wrap items-center gap-x-4 gap-y-0.5 text-label text-[var(--text-3)]"
	>
		{#each metaCells as [label, value, cellTitle] (label)}
			<span class="inline-flex items-baseline gap-1">
				<span>{label}</span>
				<span class="font-mono text-[var(--text-strong)]" title={cellTitle}>{value}</span>
			</span>
		{/each}
	</div>
</header>

<style>
	.detail-header {
		--edge-tl: 6px;
		--edge-br: 8px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		padding: calc(var(--edge-tl) + 4px) calc(var(--edge-br) + 4px) calc(var(--edge-br) + 4px)
			calc(var(--edge-tl) + 4px);
	}
</style>
