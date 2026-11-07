<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { resolve } from '$app/paths';
	import { onCacheEviction } from '$lib/stores/cache/unifiedCache.svelte';
	import {
		onDetailEviction,
		onSessionPageEvict
	} from '$lib/stores/cache/sessionDetailStore.svelte';
	import { onRequestDetailEvict } from '$lib/stores/cache/requestDetailStore.svelte';
	import { onFindingEvict } from '$lib/stores/findings.svelte';
	import { createMemoryStore, DEFAULT_POLL_MS } from '$lib/state/memory.svelte';
	import { formatBytes } from '$lib/format';
	import { formatRelative } from '$lib/time';
	import type { MemoryEntry } from '$lib/api/memory';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';

	const memory = createMemoryStore(50);

	const RSS_HISTORY_CAP = 120;
	let rssHistory = $state<Array<{ at: number; rss: number | null }>>([]);
	let lastRecordedFetchAt = 0;

	interface EvictionRecord {
		at: number;
		source: string;
		reason: string;
		detail: string;
	}

	const EVICTION_RECORDS_CAP = 200;

	const COUNTER_WINDOW_MS = 60_000;

	let evictionRecords = $state<EvictionRecord[]>([]);
	let totalEvictionsObserved = $state(0);

	function pushEviction(rec: EvictionRecord): void {
		totalEvictionsObserved += 1;
		evictionRecords.push(rec);
		if (evictionRecords.length > EVICTION_RECORDS_CAP) {
			evictionRecords.splice(0, evictionRecords.length - EVICTION_RECORDS_CAP);
		}
	}

	let unsubs: Array<() => void> = [];
	let cleanupMemory: (() => void) | null = null;
	let tickTimer: ReturnType<typeof setInterval> | null = null;
	let now = $state(Date.now());

	onMount(() => {
		cleanupMemory = memory.start(DEFAULT_POLL_MS);
		unsubs = [
			onCacheEviction((e) =>
				pushEviction({
					at: Date.now(),
					source: e.source,
					reason: e.reason,
					detail:
						`id=${e.id}` +
						('kind' in e ? ` kind=${e.kind}` : '') +
						('bytes' in e ? ` bytes=${e.bytes}` : '')
				})
			),
			onSessionPageEvict((e) =>
				pushEviction({
					at: Date.now(),
					source: 'session-pages',
					reason: e.reason,
					detail: `session=${e.sessionId} offset=${e.offset} rows=${e.rows}`
				})
			),
			onDetailEviction((e) =>
				pushEviction({
					at: Date.now(),
					source: 'session-details',
					reason: e.reason,
					detail: `session=${e.sessionId ?? '(none)'} request=${e.requestId}`
				})
			),
			onRequestDetailEvict((e) =>
				pushEviction({
					at: Date.now(),
					source: 'request-detail',
					reason: e.reason,
					detail: `id=${e.id}`
				})
			),
			onFindingEvict((e) =>
				pushEviction({
					at: Date.now(),
					source: 'findings',
					reason: e.reason,
					detail: `key=${e.findingKey}`
				})
			)
		];
		tickTimer = setInterval(() => {
			now = Date.now();
		}, 1_000);
	});

	onDestroy(() => {
		for (const u of unsubs) u();
		unsubs = [];
		if (cleanupMemory) cleanupMemory();
		cleanupMemory = null;
		if (tickTimer !== null) clearInterval(tickTimer);
		tickTimer = null;
	});

	$effect(() => {
		const fetchedAt = memory.lastFetchedAt;
		if (fetchedAt === null) return;
		if (fetchedAt === lastRecordedFetchAt) return;
		const snap = memory.snapshot;
		if (!snap) return;
		lastRecordedFetchAt = fetchedAt;
		rssHistory.push({ at: fetchedAt, rss: snap.process_rss_bytes });
		if (rssHistory.length > RSS_HISTORY_CAP) {
			rssHistory.splice(0, rssHistory.length - RSS_HISTORY_CAP);
		}
	});

	const recentCounts = $derived.by((): Map<string, number> => {
		const cutoff = now - COUNTER_WINDOW_MS;
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- local aggregation inside $derived.by; built once and returned, never mutated after recomputation
		const out = new Map<string, number>();
		for (const r of evictionRecords) {
			if (r.at < cutoff) continue;
			const key = `${r.source}:${r.reason}`;
			out.set(key, (out.get(key) ?? 0) + 1);
		}
		return out;
	});

	const sortedCounts = $derived.by(() => {
		const arr = Array.from(recentCounts.entries());
		arr.sort((a, b) => {
			if (b[1] !== a[1]) return b[1] - a[1];
			return a[0] < b[0] ? -1 : 1;
		});
		return arr;
	});

	const recentEvictions = $derived.by(() => {
		return [...evictionRecords].reverse();
	});

	const SPARK_W = 600;
	const SPARK_H = 64;
	const sparkPath = $derived.by((): { path: string; min: number; max: number } | null => {
		if (rssHistory.length === 0) return null;
		const points = rssHistory.filter((p): p is { at: number; rss: number } => p.rss !== null);
		if (points.length === 0) return null;
		let min = points[0]!.rss;
		let max = points[0]!.rss;
		for (const p of points) {
			if (p.rss < min) min = p.rss;
			if (p.rss > max) max = p.rss;
		}
		if (max - min < 1_000_000) {
			const pad = 5_000_000;
			min = Math.max(0, min - pad);
			max = max + pad;
		}
		const range = max - min;
		const x0 = points[0]!.at;
		const xN = points[points.length - 1]!.at;
		const xRange = Math.max(1, xN - x0);
		const segments: string[] = [];
		points.forEach((p, idx) => {
			const x = ((p.at - x0) / xRange) * SPARK_W;
			const y = SPARK_H - ((p.rss - min) / range) * SPARK_H;
			segments.push(`${idx === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`);
		});
		return { path: segments.join(' '), min, max };
	});

	const snapshot = $derived(memory.snapshot);
	const topEntries = $derived<MemoryEntry[]>(
		(snapshot?.top_entries ?? []).filter((e) => e.status.toLowerCase() !== 'streaming')
	);

	function toIso(at: number | string): string {
		return typeof at === 'number' ? new Date(at).toISOString() : at;
	}

	function relTime(at: number | string): string {
		return formatRelative(toIso(at), now);
	}

	function absTime(at: number | string): string {
		return new Date(at).toLocaleString('en-US');
	}
</script>

<svelte:head>
	<title>Memory diagnostics — mike</title>
</svelte:head>

<section class="memory-page">
	<PageIntro title="Memory diagnostics" learnMoreHref="/help/guides/retention">
		<span>
			Live snapshot of in-process bytes + the SPA's eviction pressure. Polls
			<code class="font-mono text-[var(--text-2)]">/api/diagnostics/memory</code> every {DEFAULT_POLL_MS /
				1000}s.
			{#if memory.lastFetchedAt !== null}
				Last snapshot
				<time datetime={toIso(memory.lastFetchedAt)} title={absTime(memory.lastFetchedAt)}
					>{relTime(memory.lastFetchedAt)}</time
				>.
			{/if}
			{#if memory.loading}
				<span class="memory-page__loading">Fetching…</span>
			{/if}
		</span>
	</PageIntro>

	{#if memory.error !== null && snapshot === null}
		<LoadErrorBanner
			title="Cannot reach inspect API"
			detail={memory.error}
			hint="The proxy's inspect API is normally reachable on :9989 — check it's still running."
			onRetry={() => memory.refresh()}
		/>
	{/if}

	<p class="text-label font-semibold uppercase tracking-kpi text-[var(--text-3)]">Overview</p>
	<div class="memory-page__grid">
		<SectionCard title="Process">
			<dl class="mem-metric-list">
				<div>
					<dt>
						<Tooltip
							text="Resident set size — the OS-measured physical RAM the whole process occupies right now (everything, not just the capture store)."
						>
							RSS (peak)
						</Tooltip>
					</dt>
					<dd>
						{snapshot?.process_rss_bytes !== null && snapshot?.process_rss_bytes !== undefined
							? formatBytes(snapshot.process_rss_bytes)
							: '—'}
					</dd>
				</div>
				<div>
					<dt>
						<Tooltip text="Requests currently streaming — captured but not yet completed.">
							In-flight requests
						</Tooltip>
					</dt>
					<dd>{snapshot?.in_flight_count ?? '—'}</dd>
				</div>
				<div>
					<dt>Resident entries</dt>
					<dd>
						{snapshot?.entry_count ?? '—'}{#if snapshot}<span class="mem-hint">
								/ {snapshot.max_entries} cap</span
							>{/if}
					</dd>
				</div>
				<div>
					<dt>
						<Tooltip
							text="The in-process cap on captured bytes (used / max). Distinct from RSS — this counts only the capture store, and bounds it; RSS is the whole process measured by the OS."
						>
							Byte budget
						</Tooltip>
					</dt>
					<dd>
						{#if snapshot}
							{#if snapshot.max_store_bytes === 0}
								<em>disabled</em>
							{:else}
								{formatBytes(snapshot.entries_total_bytes)} /
								{formatBytes(snapshot.max_store_bytes)}
								{#if snapshot.over_byte_budget}
									<span class="badge-nx badge-warning text-label">over</span>
								{/if}
							{/if}
						{:else}
							—
						{/if}
					</dd>
				</div>
			</dl>
		</SectionCard>

		<SectionCard title="Section bytes">
			<dl class="mem-metric-list">
				<div>
					<dt>
						<Tooltip text="Bytes held for the raw client request bodies forwarded upstream.">
							Request bodies
						</Tooltip>
					</dt>
					<dd>{snapshot ? formatBytes(snapshot.request_body_bytes) : '—'}</dd>
				</div>
				<div>
					<dt>
						<Tooltip
							text="Bytes held for the upstream response bodies streamed back to the client."
						>
							Response bodies
						</Tooltip>
					</dt>
					<dd>{snapshot ? formatBytes(snapshot.response_body_bytes) : '—'}</dd>
				</div>
				<div>
					<dt>
						<Tooltip
							text="Bytes for request bodies after filter rules ran — the rule-rewritten copy retained alongside the original raw body."
						>
							Filtered req bodies
						</Tooltip>
					</dt>
					<dd>{snapshot ? formatBytes(snapshot.filtered_request_body_bytes) : '—'}</dd>
				</div>
				<div>
					<dt>
						<Tooltip
							text="Bytes for tool-call argument blocks extracted from requests and stored as JSONB."
						>
							Tool calls (JSONB)
						</Tooltip>
					</dt>
					<dd>{snapshot ? formatBytes(snapshot.tool_calls_bytes) : '—'}</dd>
				</div>
				<div>
					<dt>
						<Tooltip text="Bytes for tool-result blocks captured as standalone results.">
							Tool results
						</Tooltip>
					</dt>
					<dd>{snapshot ? formatBytes(snapshot.tool_results_bytes) : '—'}</dd>
				</div>
				<div>
					<dt>
						<Tooltip
							text="Bytes for tool results that arrived inline inside a request body rather than as a separate result block."
						>
							Tool results in body
						</Tooltip>
					</dt>
					<dd>{snapshot ? formatBytes(snapshot.tool_results_in_body_bytes) : '—'}</dd>
				</div>
				<div>
					<dt>
						<Tooltip
							text="Bytes for the system-prompt blocks captured from the system field of requests."
						>
							System prompt blocks
						</Tooltip>
					</dt>
					<dd>{snapshot ? formatBytes(snapshot.system_prompt_blocks_bytes) : '—'}</dd>
				</div>
			</dl>
		</SectionCard>

		<SectionCard title="Indexes">
			<dl class="mem-metric-list">
				<div>
					<dt>
						<Tooltip
							text="Number of sessions tracked in the primary request-by-session index (one key per session id)."
						>
							Session index keys
						</Tooltip>
					</dt>
					<dd>{snapshot?.session_index_keys ?? '—'}</dd>
				</div>
				<div>
					<dt>
						<Tooltip
							text="Keys in the composable-session index used to assemble multi-request session views."
						>
							Composable session index
						</Tooltip>
					</dt>
					<dd>{snapshot?.composable_session_index_keys ?? '—'}</dd>
				</div>
				<div>
					<dt>
						<Tooltip
							text="Keys in the session-metadata index (per-session summary fields kept apart from the request bodies)."
						>
							Session meta keys
						</Tooltip>
					</dt>
					<dd>{snapshot?.session_meta_keys ?? '—'}</dd>
				</div>
			</dl>
		</SectionCard>
	</div>

	<p class="text-label font-semibold uppercase tracking-kpi text-[var(--text-3)]">Trends</p>
	<SectionCard
		title="RSS history (last {Math.round((rssHistory.length * DEFAULT_POLL_MS) / 1000)}s)"
	>
		{#snippet children()}
			{#if sparkPath}
				<p class="mem-hint">
					min {formatBytes(sparkPath.min)} · max {formatBytes(sparkPath.max)} · samples {rssHistory.length}
				</p>
				<svg
					class="memory-spark"
					viewBox={`0 0 ${SPARK_W} ${SPARK_H}`}
					preserveAspectRatio="none"
					aria-label="Process RSS over the last six minutes"
					role="img"
				>
					<path d={sparkPath.path} fill="none" stroke="currentColor" stroke-width="1.5" />
				</svg>
			{:else}
				<p class="text-xs italic text-[var(--text-3)]">Awaiting first snapshot…</p>
			{/if}
		{/snippet}
	</SectionCard>

	<p class="text-label font-semibold uppercase tracking-kpi text-[var(--text-3)]">Investigation</p>
	<SectionCard title="Largest resident requests">
		{#snippet children()}
			<p class="mem-hint">Completed requests ordered by total bytes.</p>
			{#if topEntries.length > 0}
				<div class="memory-table-wrap">
					<table class="memory-table">
						<thead>
							<tr>
								<th>Request</th>
								<th>Session</th>
								<th>Status</th>
								<th class="num">Req body</th>
								<th class="num">Resp body</th>
								<th class="num">Filt req</th>
								<th class="num">Total</th>
								<th>Created</th>
							</tr>
						</thead>
						<tbody>
							{#each topEntries as entry (entry.id)}
								<tr>
									<td>
										<a
											href={resolve(
												`sessions/${encodeURIComponent(entry.session_id)}/requests/${entry.id}`
											)}
										>
											{entry.id}
										</a>
									</td>
									<td>
										<a href={resolve(`sessions/${encodeURIComponent(entry.session_id)}`)}>
											{entry.session_id}
										</a>
									</td>
									<td>{entry.status}</td>
									<td class="num">{formatBytes(entry.request_body_bytes)}</td>
									<td class="num">{formatBytes(entry.response_body_bytes)}</td>
									<td class="num">{formatBytes(entry.filtered_request_body_bytes)}</td>
									<td class="num memory-table__total">{formatBytes(entry.total_bytes)}</td>
									<td>
										<time datetime={entry.created_at} title={absTime(entry.created_at)}
											>{relTime(entry.created_at)}</time
										>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{:else if snapshot}
				<p class="text-xs italic text-[var(--text-3)]">Store is empty.</p>
			{:else}
				<p class="text-xs italic text-[var(--text-3)]">Awaiting first snapshot…</p>
			{/if}
		{/snippet}
	</SectionCard>

	<SectionCard title="SPA eviction activity">
		{#snippet children()}
			<p class="mem-hint">
				{totalEvictionsObserved} evictions since this page mounted ·
				{evictionRecords.length} resident in tail · 5 surfaces wired
			</p>
			<div class="mem-subhead">
				<Tooltip
					text="How often the browser SPA dropped cached data to stay within its in-tab memory budget, counted per source over the last window. High counts mean the SPA is churning its caches, not that memory is leaking."
				>
					<h4 class="mem-subhead__title">Pressure (last {COUNTER_WINDOW_MS / 1000}s)</h4>
				</Tooltip>
			</div>
			{#if sortedCounts.length === 0}
				<p class="text-xs italic text-[var(--text-3)]">No evictions observed.</p>
			{:else}
				<ul class="memory-counter-list">
					{#each sortedCounts as [key, count] (key)}
						<li>
							<span class="memory-counter-list__key">{key}</span>
							<span class="memory-counter-list__count">{count}</span>
						</li>
					{/each}
				</ul>
			{/if}

			<div class="mem-subdivider" aria-hidden="true"></div>

			<div class="mem-subhead">
				<Tooltip
					text="Newest first. Buffer caps at {EVICTION_RECORDS_CAP} entries — older records roll out as new ones arrive at the head; the total counter above tracks everything observed since mount."
				>
					<h4 class="mem-subhead__title">Recent evictions</h4>
				</Tooltip>
			</div>
			{#if recentEvictions.length === 0}
				<p class="text-xs italic text-[var(--text-3)]">No evictions observed.</p>
			{:else}
				<div class="memory-table-wrap memory-table-wrap--feed">
					<table class="memory-table memory-table--feed">
						<thead>
							<tr>
								<th>Time</th>
								<th>Source</th>
								<th>Reason</th>
								<th>Detail</th>
							</tr>
						</thead>
						<tbody>
							{#each recentEvictions as rec (rec.at + ':' + rec.source + ':' + rec.detail)}
								<tr>
									<td>
										<time datetime={toIso(rec.at)} title={absTime(rec.at)}>{relTime(rec.at)}</time>
									</td>
									<td>{rec.source}</td>
									<td>{rec.reason}</td>
									<td class="mono">{rec.detail}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		{/snippet}
	</SectionCard>
</section>

<style>
	.memory-page {
		padding: 16px;
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 16px;
		max-width: 1200px;
		margin: 0 auto;
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
	}

	.memory-page__loading {
		font-style: italic;
		color: var(--text-faint);
		margin-left: 8px;
	}

	.memory-page__grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
		gap: 12px;
	}

	.mem-hint {
		margin: 0 0 8px;
		font-size: var(--text-caption);
		color: var(--text-faint);
	}

	.mem-metric-list {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 4px 12px;
		font-size: 12px;
		margin: 0;
	}

	.mem-metric-list > div {
		display: contents;
	}

	.mem-metric-list dt {
		color: var(--text-3);
	}

	.mem-metric-list dd {
		text-align: right;
		font-variant-numeric: tabular-nums;
		color: var(--text-strong);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		margin: 0;
	}

	.memory-spark {
		display: block;
		width: 100%;
		height: 64px;
		color: var(--interactive-active-fg);
	}

	.mem-subhead {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
	}

	.mem-subhead + .memory-counter-list,
	.mem-subhead + p,
	.mem-subhead + .memory-table-wrap {
		margin-top: 8px;
	}

	.mem-subhead__title {
		margin: 0;
		font-size: 12px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}

	.mem-subdivider {
		border-top: 1px solid var(--border-default);
		margin: 16px 0;
	}

	.memory-table-wrap {
		overflow-x: auto;
	}

	.memory-table-wrap--feed {
		max-height: 360px;
		overflow-y: auto;
		border-top: 1px solid var(--border-default);
	}

	.memory-table {
		width: 100%;
		font-size: 12px;
		border-collapse: collapse;
	}

	.memory-table th {
		text-align: left;
		font-weight: 600;
		color: var(--text-3);
		padding: 6px 8px;
		border-bottom: 1px solid var(--border-default);
		text-transform: uppercase;
		font-size: 10px;
		letter-spacing: 0.04em;
		position: sticky;
		top: 0;
		background: var(--surface-card);
	}

	.memory-table th.num {
		text-align: right;
	}

	.memory-table td {
		padding: 6px 8px;
		border-bottom: 1px solid var(--border-default);
		color: var(--text-2);
	}

	.memory-table td.num {
		text-align: right;
		font-variant-numeric: tabular-nums;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
	}

	.memory-table__total {
		color: var(--text-strong);
		font-weight: 600;
	}

	.memory-table a {
		color: var(--interactive-active-fg);
		text-decoration: none;
	}

	.memory-table a:hover {
		text-decoration: underline;
	}

	.memory-table .mono {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-3);
	}

	.memory-counter-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
		gap: 4px 12px;
	}

	.memory-counter-list li {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		font-size: 12px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		padding: 2px 0;
	}

	.memory-counter-list__key {
		color: var(--text-2);
	}

	.memory-counter-list__count {
		color: var(--interactive-active-fg);
		font-variant-numeric: tabular-nums;
		font-weight: 600;
	}
</style>
