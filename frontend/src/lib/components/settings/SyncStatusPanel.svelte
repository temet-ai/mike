<script lang="ts">
	import { formatRelative, parseIso, formatCount } from '$lib/time';
	import type { SyncStatusDto } from '$lib/api/settings';

	interface Props {
		status: SyncStatusDto;
		now?: number;
	}

	let { status, now }: Props = $props();

	const hasRun = $derived(status.last_run_at !== null);

	const lastRunRelative = $derived(
		status.last_run_at !== null
			? formatRelative(status.last_run_at, now ?? Date.now())
			: 'never run'
	);
	const lastRunIso = $derived(
		status.last_run_at !== null
			? (parseIso(status.last_run_at)?.toISOString() ?? status.last_run_at)
			: null
	);

	const tables = $derived(status.last_report.tables);
	const totalRows = $derived(status.last_report.total_rows);
</script>

<div class="sync-panel" data-augmented-ui="tl-clip br-clip border">
	<div class="sync-header">
		<h2 class="sync-title" id="sync-status-title">Local DB resync</h2>
		<span
			class="sync-reach"
			class:sync-reach--ok={status.dest_reachable}
			class:sync-reach--down={!status.dest_reachable}
		>
			<span class="sync-reach-dot" aria-hidden="true"></span>
			{status.dest_reachable ? 'Remote DB reachable' : 'Remote DB unreachable'}
		</span>
	</div>

	<p class="sync-desc">
		The resync worker merges rows captured to the local fallback database back into the remote
		database when it is reachable. This is a read-only view of its most recent run.
	</p>

	<div class="sync-row">
		<span class="sync-row-label">Last run</span>
		{#if lastRunIso !== null}
			<span class="sync-row-value" title={lastRunIso}>
				{lastRunRelative}
				<span class="sync-row-iso">({lastRunIso})</span>
			</span>
		{:else}
			<span class="sync-row-value sync-row-value--muted">never run</span>
		{/if}
	</div>

	<div class="sync-summary">
		<div class="sync-row">
			<span class="sync-row-label">Last sync</span>
			<span class="sync-row-value">
				{formatCount(totalRows)}
				{totalRows === 1 ? 'row' : 'rows'} synced
			</span>
		</div>

		{#if tables.length > 0}
			<ul class="sync-table-list" aria-label="Per-table rows synced on the last run">
				{#each tables as [table, inserted] (table)}
					<li class="sync-table-item">
						<span class="sync-table-name">{table}</span>
						<span class="sync-table-count">{formatCount(inserted)}</span>
					</li>
				{/each}
			</ul>
		{:else if hasRun}
			<p class="sync-note">No rows needed syncing on the last run.</p>
		{/if}
	</div>

	{#if status.last_error !== null}
		<div class="sync-error" role="alert">
			<span class="sync-error-label">Last error</span>
			<p class="sync-error-msg">{status.last_error}</p>
		</div>
	{/if}

	{#if !hasRun && status.last_error === null}
		<p class="sync-note sync-note--empty">
			The resync worker has not run yet. It runs when both the local and remote databases are
			reachable; until then there is nothing to report.
		</p>
	{/if}
</div>

<style>
	.sync-panel {
		--edge: 10px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-info-fg) 0%,
			var(--status-info-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: calc(var(--edge) + 6px) calc(var(--edge) + 10px);
		background: var(--surface-card);
	}

	.sync-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		flex-wrap: wrap;
	}

	.sync-title {
		font-family: var(--font-display);
		font-size: 13px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 0;
	}

	.sync-reach {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 2px 8px;
		border-radius: 4px;
		font-family: inherit;
		font-size: 11px;
		font-weight: 600;
		white-space: nowrap;
	}

	.sync-reach--ok {
		background: var(--status-success-bg);
		color: var(--status-success-fg);
		border: 1px solid var(--status-success-border);
	}

	.sync-reach--down {
		background: var(--status-warning-bg);
		color: var(--status-warning-fg);
		border: 1px solid var(--status-warning-border);
	}

	.sync-reach-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: currentColor;
		flex-shrink: 0;
	}

	.sync-desc {
		margin: 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.6;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sync-row {
		display: flex;
		align-items: baseline;
		gap: 10px;
		flex-wrap: wrap;
	}

	.sync-row-label {
		font-family: inherit;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
		flex-shrink: 0;
		min-width: 70px;
	}

	.sync-row-value {
		font-family: inherit;
		font-size: 13px;
		color: var(--text-strong);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sync-row-value--muted {
		color: var(--text-3);
	}

	.sync-row-iso {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-3);
	}

	.sync-summary {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
		background: var(--surface-2);
	}

	.sync-table-list {
		display: flex;
		flex-direction: column;
		gap: 0;
		margin: 0;
		padding: 0;
		list-style: none;
		max-height: 320px;
		overflow-y: auto;
		border-top: 1px solid var(--border-subtle);
	}

	.sync-table-item {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		padding: 5px 0;
		border-bottom: 1px solid var(--border-subtle);
	}

	.sync-table-item:last-child {
		border-bottom: none;
	}

	.sync-table-name {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-1);
		overflow-wrap: break-word;
		word-break: break-word;
		min-width: 0;
	}

	.sync-table-count {
		font-family: var(--font-mono);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-strong);
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
	}

	.sync-note {
		margin: 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-3);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sync-note--empty {
		padding: 10px 12px;
		border: 1px dashed var(--border-default);
		border-radius: 6px;
		background: var(--surface-2);
	}

	.sync-error {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 10px 12px;
		border: 1px solid var(--status-danger-border);
		border-left: 2px solid var(--status-danger-border);
		border-radius: 6px;
		background: var(--status-danger-bg);
	}

	.sync-error-label {
		font-family: inherit;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--status-danger-fg);
	}

	.sync-error-msg {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}
</style>
