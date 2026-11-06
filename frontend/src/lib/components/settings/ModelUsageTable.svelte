<script lang="ts">
	import { formatClockAbsolute, formatCompactCount, formatRelative } from '$lib/time';
	import type { ModelUsageRow } from '$lib/api/usage';

	interface Props {
		rows: ModelUsageRow[] | null;
		error: string | null;
		unavailable: boolean;
		days: number;
		onRetry?: () => void;
	}

	const { rows, error, unavailable, days, onRetry }: Props = $props();

	function exact(value: number): string {
		return value.toLocaleString('en-US');
	}
</script>

<section class="mut-section" aria-labelledby="mut-title">
	<div class="mut-card" data-augmented-ui="tl-clip br-clip border">
		<div class="mut-header">
			<h2 class="mut-title" id="mut-title">Models that have run (last {days} days)</h2>
		</div>
		<p class="mut-desc">
			Every model observed in captured traffic over the trailing window, ordered by request volume.
		</p>

		<div class="mut-body">
			{#if unavailable}
				<p class="mut-unavailable-msg">
					This table needs a newer version than the one currently running.
				</p>
			{:else if error !== null}
				<div class="mut-error" role="alert" data-augmented-ui="tl-clip br-clip border">
					<p class="mut-error-msg">Could not load model usage: {error}</p>
					{#if onRetry}
						<span class="mut-retry-backing"
							><button
								type="button"
								class="mut-retry"
								data-augmented-ui="tr-clip bl-clip border"
								onclick={() => onRetry?.()}>Retry</button
							></span
						>
					{/if}
				</div>
			{:else if rows === null}
				<p class="mut-loading" aria-busy="true">Loading model usage…</p>
			{:else if rows.length === 0}
				<p class="mut-empty-msg">No traffic captured in the last {days} days.</p>
			{:else}
				<div class="mut-scroll">
					<table class="mut-table" data-testid="model-usage-table">
						<thead>
							<tr>
								<th class="mut-th mut-th--model" scope="col">Model</th>
								<th class="mut-th mut-th--num" scope="col">Requests</th>
								<th class="mut-th mut-th--num" scope="col">Input tokens</th>
								<th class="mut-th mut-th--num" scope="col">Output tokens</th>
								<th class="mut-th mut-th--num" scope="col">Cache write</th>
								<th class="mut-th mut-th--num" scope="col">Cache read</th>
								<th class="mut-th mut-th--num" scope="col">Last seen</th>
							</tr>
						</thead>
						<tbody>
							{#each rows as row, i (row.model !== null ? `${row.model}-${i}` : `unknown-${i}`)}
								<tr class="mut-row">
									<td class="mut-td mut-td--model">
										{#if row.model === null || row.model === ''}
											<span class="mut-unknown">(unknown model)</span>
										{:else}
											{row.model}
										{/if}
									</td>
									<td class="mut-td mut-td--num" title={exact(row.requests)}
										>{formatCompactCount(row.requests)}</td
									>
									<td class="mut-td mut-td--num" title={exact(row.input_tokens)}
										>{formatCompactCount(row.input_tokens)}</td
									>
									<td class="mut-td mut-td--num" title={exact(row.output_tokens)}
										>{formatCompactCount(row.output_tokens)}</td
									>
									<td class="mut-td mut-td--num" title={exact(row.cache_creation_tokens)}
										>{formatCompactCount(row.cache_creation_tokens)}</td
									>
									<td class="mut-td mut-td--num" title={exact(row.cache_read_tokens)}
										>{formatCompactCount(row.cache_read_tokens)}</td
									>
									<td class="mut-td mut-td--last" title={formatClockAbsolute(row.last_seen)}
										>{formatRelative(row.last_seen)}</td
									>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</div>
	</div>
</section>

<style>
	.mut-section {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.mut-card {
		--edge-tl: 8px;
		--edge-br: 10px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(135deg, var(--border-strong) 0%, var(--border-default) 60%);
		display: flex;
		flex-direction: column;
		background: var(--surface-card);
	}
	.mut-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 16px 16px 0;
	}
	.mut-title {
		font-family: var(--font-display);
		font-size: 13px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 0;
	}
	.mut-desc {
		margin: 6px 0 0;
		padding: 0 16px;
		font-size: 12px;
		color: var(--text-3);
		line-height: 1.55;
	}
	.mut-body {
		padding: 14px 16px 16px;
		border-top: 1px solid var(--border-subtle);
		margin-top: 14px;
	}

	.mut-unavailable-msg {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
		line-height: 1.55;
	}
	.mut-loading {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
		line-height: 1.55;
	}
	.mut-empty-msg {
		margin: 0;
		font-size: 13px;
		color: var(--text-2);
		line-height: 1.6;
	}

	.mut-error {
		--edge: 10px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-danger-fg) 0%,
			var(--status-danger-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: calc(var(--edge) + 6px) calc(var(--edge) + 10px);
		background: var(--status-danger-bg);
	}
	.mut-error-msg {
		margin: 0;
		font-size: 13px;
		color: var(--status-danger-fg);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.mut-retry-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--status-danger-bg);
	}
	.mut-retry {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: 6px 14px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 12px;
		cursor: pointer;
		transition: color 100ms ease-out;
	}
	.mut-retry:hover {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}

	.mut-scroll {
		overflow-x: auto;
		border: 1px solid var(--border-subtle);
		background: var(--surface-card);
	}

	.mut-table {
		width: 100%;
		border-collapse: collapse;
		table-layout: auto;
		font-family: inherit;
		font-size: 12px;
	}

	.mut-th {
		padding: 8px 12px;
		text-align: right;
		font-family: inherit;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-2);
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
	}
	.mut-th--model {
		text-align: left;
		min-width: 220px;
	}

	.mut-row {
		border-bottom: 1px solid var(--border-subtle);
	}
	.mut-row:last-child {
		border-bottom: none;
	}
	.mut-row:hover {
		background: var(--surface-2);
	}

	.mut-td {
		padding: 9px 12px;
		color: var(--text-1);
		vertical-align: top;
	}
	.mut-td--model {
		overflow-wrap: anywhere;
		word-break: break-word;
	}
	.mut-unknown {
		color: var(--text-3);
		font-style: italic;
	}
	.mut-td--num {
		text-align: right;
		white-space: nowrap;
		color: var(--text-1);
		font-variant-numeric: tabular-nums;
	}
	.mut-td--last {
		text-align: right;
		white-space: nowrap;
		color: var(--text-2);
	}
</style>
