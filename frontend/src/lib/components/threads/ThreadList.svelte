<!--
@component
The threads of one project, newest change first. Each row is a link that opens the thread beside
the list; the status filter narrows the list through the URL.
-->
<script lang="ts">
	import Badge from '$lib/components/shared/Badge.svelte';
	import { formatDay } from '$lib/components/eval/evalReview';
	import type { ThreadListRow } from '$lib/api/workThreads';
	import { statusLabel, statusVariant } from './threadFormat';

	let {
		threads,
		selected,
		hrefFor
	}: {
		threads: ThreadListRow[];
		selected: string | null;
		hrefFor: (slug: string) => string;
	} = $props();
</script>

<nav class="tl" aria-label="Threads" data-slot="thread-list">
	{#each threads as thread (thread.slug)}
		<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- hrefFor builds the route with resolve() in the page -->
		<a
			class="tl-row"
			href={hrefFor(thread.slug)}
			aria-current={thread.slug === selected ? 'page' : undefined}
			data-selected={thread.slug === selected ? '' : undefined}
		>
			<span class="tl-head">
				<span class="tl-title">{thread.title ?? thread.slug}</span>
				<Badge variant={statusVariant(thread.status)}>{statusLabel(thread.status)}</Badge>
			</span>
			<span class="tl-meta"
				>{thread.slug} · {formatDay(thread.updated_at) ?? thread.updated_at}</span
			>
			{#if thread.tldr}
				<span class="tl-tldr">{thread.tldr}</span>
			{/if}
			{#if thread.parse_error}
				<span class="tl-error">Import problem: {thread.parse_error}</span>
			{/if}
		</a>
	{/each}
</nav>

<style>
	.tl {
		display: flex;
		flex-direction: column;
		background: var(--surface-card);
		border: 1px solid var(--border-default);
	}
	.tl-row {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-4);
		border-bottom: 1px solid var(--border-subtle);
		color: var(--text-1);
		text-decoration: none;
	}
	.tl-row:last-child {
		border-bottom: 0;
	}
	.tl-row:hover {
		background: var(--surface-2);
	}
	.tl-row[data-selected] {
		background: var(--surface-2);
		box-shadow: inset 0 0 0 1px var(--border-strong);
	}
	.tl-row:focus-visible {
		outline: 2px solid var(--ring);
		outline-offset: -2px;
	}
	.tl-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
		flex-wrap: wrap;
	}
	.tl-title {
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.tl-meta {
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
	.tl-tldr {
		font-size: var(--text-xs);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.tl-error {
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
</style>
