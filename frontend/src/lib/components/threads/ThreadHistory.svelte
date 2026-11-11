<!--
@component
The append-only history of one thread, newest first. The section header and each entry header
toggle on click; the whole history starts collapsed and the choice is remembered in the browser.
Entry bodies are shown whole.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import Badge from '$lib/components/shared/Badge.svelte';
	import { formatDay } from '$lib/components/eval/evalReview';
	import type { ThreadEntry } from '$lib/api/workThreads';

	let { entries, total }: { entries: ThreadEntry[]; total: number } = $props();

	const STORAGE_KEY = 'mike.threads.history-open';

	function readOpen(): boolean {
		try {
			return localStorage.getItem(STORAGE_KEY) === '1';
		} catch {
			return false;
		}
	}

	let open = $state(readOpen());
	let expanded = $state<Record<number, boolean>>({});

	function toggleSection(): void {
		open = !open;
		try {
			localStorage.setItem(STORAGE_KEY, open ? '1' : '0');
		} catch {
			// The choice is a convenience; without storage it lasts for this view only.
		}
	}

	function toggleEntry(ord: number): void {
		expanded = { ...expanded, [ord]: !expanded[ord] };
	}
</script>

<section class="th" aria-label="History" data-slot="thread-history">
	<button type="button" class="th-head" aria-expanded={open} onclick={toggleSection}>
		<h3 class="th-title">History</h3>
		<span class="th-count">{total} {total === 1 ? 'entry' : 'entries'}</span>
	</button>
	{#if open}
		<ol class="th-list">
			{#each entries as entry (entry.ord)}
				<li class="th-entry">
					<button
						type="button"
						class="th-entry-head"
						aria-expanded={expanded[entry.ord] === true}
						onclick={() => toggleEntry(entry.ord)}
					>
						<Badge variant="tag-default">{entry.kind}</Badge>
						<span class="th-date">{formatDay(entry.entry_date) ?? entry.entry_date}</span>
						<span class="th-summary">{entry.summary ?? 'No summary'}</span>
					</button>
					{#if expanded[entry.ord]}
						<div class="th-body">
							<pre class="th-text">{entry.body}</pre>
							<p class="th-facts">
								{#if entry.session_uuid}
									Session
									<a href={resolve('/sessions/[sessionId]', { sessionId: entry.session_uuid })}
										>{entry.session_uuid}</a
									>
								{/if}
								{#if entry.commit_sha}
									Commit <code>{entry.commit_sha}</code>
								{/if}
							</p>
						</div>
					{/if}
				</li>
			{/each}
		</ol>
	{/if}
</section>

<style>
	.th {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.th-head {
		display: flex;
		align-items: baseline;
		gap: var(--space-3);
		padding: 0;
		background: none;
		border: 0;
		text-align: left;
		cursor: pointer;
		color: inherit;
	}
	.th-head:focus-visible,
	.th-entry-head:focus-visible {
		outline: 2px solid var(--ring);
		outline-offset: 2px;
	}
	.th-title {
		margin: 0;
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
	}
	.th-count {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.th-list {
		display: flex;
		flex-direction: column;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.th-entry {
		border-top: 1px solid var(--border-subtle);
		padding: var(--space-2) 0;
	}
	.th-entry-head {
		display: flex;
		align-items: baseline;
		gap: var(--space-3);
		flex-wrap: wrap;
		width: 100%;
		padding: 0;
		background: none;
		border: 0;
		text-align: left;
		cursor: pointer;
		color: inherit;
	}
	.th-date {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.th-summary {
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.th-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-2) 0 var(--space-1);
	}
	.th-text {
		margin: 0;
		padding: var(--space-2) var(--space-3);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-1);
		background: var(--surface-2);
		border: 1px solid var(--border-default);
	}
	.th-facts {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
</style>
