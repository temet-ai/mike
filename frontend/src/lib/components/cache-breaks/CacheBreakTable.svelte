<script lang="ts">
	import { resolve } from '$app/paths';
import type { ResolvedPathname } from '$app/types';
	import type { CacheBreak } from '$lib/api/cacheBreaks';
	import { shortSessionId } from '$lib/format';
	import { formatClockLabel } from '$lib/time';
	import { Root as DataTable, Row, Cell } from '$lib/components/ui/data-table';
	import type { DataTableColumn } from '$lib/components/ui/data-table';
	import {
		COLUMN_TOOLTIPS,
		causeLabel,
		detailNotes,
		formatGap,
		formatTokens,
		locationLabel
	} from './cacheBreakFormat';

	interface Props {
		breaks: CacheBreak[];
		now: number;
		showSession?: boolean;
		label?: string;
	}

	let { breaks, now, showSession = false, label = 'Cache breaks' }: Props = $props();

	const columns = $derived<DataTableColumn[]>([
		{ key: 'when', label: 'When' },
		...(showSession ? [{ key: 'session', label: 'Session' }] : []),
		{ key: 'kind', label: 'Kind', tooltip: COLUMN_TOOLTIPS.kind },
		{ key: 'cause', label: 'Cause', tooltip: COLUMN_TOOLTIPS.cause },
		{ key: 'where', label: 'Where', tooltip: COLUMN_TOOLTIPS.where },
		{ key: 'tokens', label: 'Tokens rewritten', align: 'end', tooltip: COLUMN_TOOLTIPS.tokens },
		{ key: 'gap', label: 'Gap', align: 'end', tooltip: COLUMN_TOOLTIPS.gap },
		{ key: 'requests', label: 'Requests' }
	]);

	function requestHref(sessionId: string, requestId: number): ResolvedPathname {
		return resolve(
			`sessions/${encodeURIComponent(sessionId)}/requests/${requestId}` as `sessions/${string}/requests/${string}`
		);
	}

	function sessionHref(sessionId: string): ResolvedPathname {
		return resolve(`sessions/${encodeURIComponent(sessionId)}` as `sessions/${string}`);
	}
</script>

<DataTable {label} {columns}>
	<tbody>
		{#each breaks as item (item.id)}
			{@const notes = detailNotes(item.detail)}
			<Row data-break-id={item.id}>
				<Cell mono>
					<time datetime={item.occurred_at} title={item.occurred_at}
						>{formatClockLabel(item.occurred_at, now)}</time
					>
				</Cell>
				{#if showSession}
					<Cell>
						<a class="cbt-link" href={sessionHref(item.session_id)}
							>{item.session_title ?? shortSessionId(item.session_id)}</a
						>
					</Cell>
				{/if}
				<Cell mono>{item.kind}</Cell>
				<Cell>
					<span class="cbt-cause">
						<span class="cbt-mono">{causeLabel(item)}</span>
						{#each notes as note (note)}
							<span class="cbt-note">{note}</span>
						{/each}
					</span>
				</Cell>
				<Cell mono>{locationLabel(item.location)}</Cell>
				<Cell align="end" mono>{formatTokens(item.tokens_rewritten)}</Cell>
				<Cell align="end" mono>{formatGap(item.gap_seconds)}</Cell>
				<Cell mono>
					<span class="cbt-requests">
						<a class="cbt-link" href={requestHref(item.session_id, item.request_id)}
							>request #{item.request_id}</a
						>
						{#if item.source_request_id !== null}
							<a class="cbt-link" href={requestHref(item.session_id, item.source_request_id)}
								>source #{item.source_request_id}</a
							>
						{/if}
					</span>
				</Cell>
			</Row>
		{/each}
	</tbody>
</DataTable>

<style>
	.cbt-cause,
	.cbt-requests {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.cbt-mono {
		font-family: var(--font-mono, monospace);
	}
	.cbt-note {
		font-size: var(--text-caption);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
	.cbt-link {
		color: var(--interactive-active-fg);
		text-decoration: none;
	}
	.cbt-link:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.cbt-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
