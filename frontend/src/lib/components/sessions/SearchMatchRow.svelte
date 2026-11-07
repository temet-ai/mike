<script lang="ts">
	import { Layers, FileText } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { shortSessionId } from '$lib/format';
	import type { SearchMatchSnippet } from '$lib/types';

	interface Props {
		sessionId: string;
		match: SearchMatchSnippet;
	}

	let { sessionId, match }: Props = $props();

	const sessionHref = $derived(`sessions/${encodeURIComponent(sessionId)}` as const);
	const requestHref = $derived(
		`sessions/${encodeURIComponent(sessionId)}/requests/${match.request_id}` as const
	);
</script>

<li class="sbm-row">
	<div class="sbm-snippet">
		<span class="sbm-badge" title={match.provenanceTitle}>{match.provenance}</span>
		<span class="sbm-text">
			<span class="sbm-ctx">{match.before}</span><mark class="sbm-mark">{match.matched}</mark><span
				class="sbm-ctx">{match.after}</span
			>
		</span>
	</div>

	<div class="sbm-meta">
		<span class="sbm-loc" title="Session {sessionId}">{shortSessionId(sessionId)}</span>
		<span class="sbm-sep" aria-hidden="true">·</span>
		<span class="sbm-loc" title="Match position in the body">L{match.line}:{match.column}</span>
		<span class="sbm-sep" aria-hidden="true">·</span>
		<span class="sbm-loc">req {match.request_id}</span>

		<span class="sbm-nav">
			<a class="sbm-link" data-motion="hover" href={resolve(sessionHref)} title="Open the session">
				<Layers size={12} aria-hidden="true" />
				<span>Session</span>
			</a>
			<a
				class="sbm-link sbm-link--primary"
				data-motion="hover"
				href={resolve(requestHref)}
				title="Open this request"
			>
				<FileText size={12} aria-hidden="true" />
				<span>Request</span>
			</a>
		</span>
	</div>
</li>

<style>
	.sbm-row {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 14px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		box-shadow: inset 0 1px 0 var(--surface-shine);
	}

	.sbm-snippet {
		display: flex;
		align-items: flex-start;
		gap: 10px;
	}
	.sbm-badge {
		flex-shrink: 0;
		margin-top: 1px;
		padding: 2px 7px;
		border-radius: 2px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-card);
		font-family: var(--font-mono);
		font-size: 10px;
		font-weight: 500;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-3);
		white-space: nowrap;
		line-height: 1.4;
	}
	.sbm-text {
		min-width: 0;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.6;
		white-space: pre-wrap;
		word-break: break-word;
		overflow-wrap: anywhere;
	}
	.sbm-ctx {
		color: var(--text-3);
	}
	.sbm-mark {
		background: var(--amber-warm);
		color: var(--text-strong);
		border-radius: 2px;
		padding: 0 2px;
		font-weight: 600;
	}

	.sbm-meta {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}
	.sbm-loc {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-faint);
		white-space: nowrap;
	}
	.sbm-sep {
		font-size: 11px;
		color: var(--text-faint);
	}

	.sbm-nav {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		margin-left: auto;
	}
	.sbm-link {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 3px 9px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-card);
		color: var(--text-2);
		font-family: var(--font-mono);
		font-size: 11px;
		text-decoration: none;
		white-space: nowrap;
	}
	.sbm-link:hover {
		color: var(--text-strong);
		border-color: var(--border-default);
	}
	.sbm-link--primary:hover {
		color: var(--interactive-active-fg);
		border-color: var(--interactive-active-border);
	}
	.sbm-link:focus-visible {
		outline: 2px solid var(--interactive-active-fg);
		outline-offset: 2px;
	}
</style>
