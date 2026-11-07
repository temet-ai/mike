<script module lang="ts">
	import type { ToolUsesFeedRow } from '$lib/types';

	export interface ToolProjectGroupRow {
		key: string;
		project: string;
		isUngrouped: boolean;
		name: string;
		path: string;
		rows: ToolUsesFeedRow[];
		failedCount: number;
		colorClass: string;
	}
</script>

<script lang="ts">
	import { resolve } from '$app/paths';
	import { slide } from 'svelte/transition';
	import { formatRelative } from '$lib/time';
	import { formatBytes, shortSessionId } from '$lib/format';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props {
		group: ToolProjectGroupRow;
		now: number;
		collapsed: boolean;
		onToggleCollapse: () => void;
		isRowExpanded: (key: string) => boolean;
		onToggleRow: (key: string) => void;
		hasRowOpened: (key: string) => boolean;
	}

	let { group, now, collapsed, onToggleCollapse, isRowExpanded, onToggleRow, hasRowOpened }: Props =
		$props();

	function rowKey(row: ToolUsesFeedRow): string {
		return `tu:${row.use_id}`;
	}

	function inputJson(input: unknown): string {
		try {
			return JSON.stringify(input, null, 2);
		} catch {
			return String(input);
		}
	}

	function contentText(value: unknown): string {
		if (typeof value === 'string') return value;
		try {
			return JSON.stringify(value, null, 2);
		} catch {
			return String(value);
		}
	}

	const FILE_TOOLS: ReadonlySet<string> = new Set([
		'Edit',
		'Write',
		'MultiEdit',
		'NotebookEdit',
		'NotebookEditCell'
	]);

	function nameKindClass(name: string): string {
		if (FILE_TOOLS.has(name)) return 'tpg-tool--file';
		if (name === 'Bash') return 'tpg-tool--bash';
		if (name === 'Task') return 'tpg-tool--task';
		return '';
	}

	function isFileTool(name: string): boolean {
		return FILE_TOOLS.has(name);
	}

	const countsLabel = $derived.by(() => {
		const n = group.rows.length;
		const calls = `${n.toLocaleString()} call${n === 1 ? '' : 's'}`;
		if (group.failedCount === 0) return calls;
		return `${calls}, ${group.failedCount.toLocaleString()} failed`;
	});
</script>

<div class="tpg-project" data-reveal>
	<div class="tpg-head" data-augmented-ui="tl-clip border" data-collapsed={collapsed}>
		<span class="tpg-chip {group.colorClass}" aria-hidden="true"></span>
		{#if !group.isUngrouped && group.path}
			<a
				class="tpg-name tpg-name--link"
				href={resolve(`projects/${encodeURIComponent(group.path)}` as const)}
			>
				{group.name}
			</a>
		{:else}
			<span class="tpg-name">{group.name}</span>
		{/if}
		<button
			type="button"
			class="tpg-toggle"
			aria-expanded={!collapsed}
			aria-controls="tpg-body-{group.key}"
			aria-label="{collapsed ? 'Expand' : 'Collapse'} {group.name}"
			onclick={onToggleCollapse}
		>
			{#if group.path}
				<span class="tpg-path">{group.path}</span>
			{/if}
			<span class="tpg-fill"></span>
		</button>
		<Tooltip text="Counts of rows loaded into this view, not the project's full history.">
			<span class="tpg-counts">{countsLabel}</span>
		</Tooltip>
	</div>

	{#if !collapsed}
		<div
			class="tpg-table"
			transition:slide={{ duration: 240 }}
			id="tpg-body-{group.key}"
			data-augmented-ui="bl-clip br-clip border"
		>
			<ul class="tpg-list" role="list">
				{#each group.rows as row (row.use_id)}
					{@const key = rowKey(row)}
					{@const isOpen = isRowExpanded(key)}
					{@const bodyId = `tpg-row-body-${row.use_id}`}
					{@const errored = row.is_error === true}
					<li
						class="tpg-row"
						class:tpg-row--file={isFileTool(row.tool_name)}
						class:tpg-row--failed={errored}
						class:tpg-row--expanded={isOpen}
					>
						<div class="tpg-row__head">
							<button
								type="button"
								class="tpg-row__disclose"
								aria-expanded={isOpen}
								aria-controls={bodyId}
								aria-label={isOpen
									? `Collapse details for ${row.tool_name}`
									: `Expand details for ${row.tool_name}`}
								onclick={() => onToggleRow(key)}
							>
								<DisclosureMarker open={isOpen} size={12} />
								<span class="tpg-tool-backing {nameKindClass(row.tool_name)}">
									<span class="tpg-tool" data-augmented-ui="tl-clip br-clip border"
										>{row.tool_name}</span
									>
								</span>
								{#if errored}
									<span class="tpg-failed-backing">
										<span class="tpg-failed" data-augmented-ui="tl-clip br-clip border">Failed</span
										>
									</span>
								{/if}
								{#if row.file_path}
									<span class="tpg-file">{row.file_path}</span>
								{/if}
								<span class="tpg-ts" title={row.created_at}
									>{formatRelative(row.created_at, now)}</span
								>
							</button>
							<div class="tpg-row__meta">
								<a
									class="tpg-link"
									href={resolve(`sessions/${encodeURIComponent(row.session_id)}`)}
									aria-label="Open session {row.session_id}"
								>
									{shortSessionId(row.session_id)}
								</a>
								<a
									class="tpg-link"
									href={resolve(
										`sessions/${encodeURIComponent(row.session_id)}/requests/${row.request_id}`
									)}
									aria-label="Open source request #{row.request_id}"
								>
									req #{row.request_id}
								</a>
							</div>
						</div>
						<div class="tpg-row__body" class:tpg-row__body--open={isOpen} id={bodyId}>
							<div class="tpg-row__body-clip" inert={!isOpen}>
								{#if hasRowOpened(key)}
									{#if errored}
										<div class="tpg-section">
											<div class="tpg-section__head">
												<span class="tpg-section__label">Error</span>
												<span class="tpg-meta">
													{row.error_content_bytes != null
														? formatBytes(row.error_content_bytes)
														: '—'}
												</span>
												<Tooltip
													text="error content sha256: {row.error_content_sha256 ?? '(none)'}"
												>
													<span class="tpg-meta tpg-meta--hash">sha256</span>
												</Tooltip>
											</div>
											<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
											<pre
												class="tpg-pre tpg-pre--error"
												tabindex="0"
												role="region"
												aria-label="Tool error content for {row.tool_name}, request #{row.request_id} (scrollable)">{contentText(
													row.error_content
												)}</pre>
										</div>
										<div class="tpg-section">
											<span class="tpg-section__label">Input</span>
											<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
											<pre
												class="tpg-pre"
												tabindex="0"
												role="region"
												aria-label="Tool input JSON for {row.tool_name}, request #{row.request_id} (scrollable)">{inputJson(
													row.input
												)}</pre>
										</div>
									{:else}
										<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
										<pre
											class="tpg-pre"
											tabindex="0"
											role="region"
											aria-label="Tool input JSON for {row.tool_name}, request #{row.request_id} (scrollable)">{inputJson(
												row.input
											)}</pre>
									{/if}
								{/if}
							</div>
						</div>
					</li>
				{/each}
			</ul>
		</div>
	{/if}
</div>

<style>
	.tpg-project {
		min-width: 0;
	}

	@property --tpg-head-clip {
		syntax: '<length>';
		inherits: true;
		initial-value: 14px;
	}
	.tpg-head {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
		padding: 3px 4px 5px 18px;
		color: var(--text-1);
		--tpg-head-clip: 14px;
		--aug-tl: var(--tpg-head-clip);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
	}
	.tpg-head[data-collapsed='true'] {
		--tpg-head-clip: 4px;
	}
	.tpg-head {
		transition: --tpg-head-clip var(--dur-2) var(--ease-io);
	}
	.tpg-toggle {
		display: flex;
		align-items: center;
		gap: 10px;
		flex: 1 1 auto;
		min-width: 0;
		text-align: left;
		background: transparent;
		border: 0;
		padding: 0;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}
	.tpg-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}
	.tpg-chip {
		width: 7px;
		height: 7px;
		border-radius: 2px;
		flex-shrink: 0;
		background: var(--text-faint);
	}
	.tpg-chip--1 {
		background: var(--tpg-chip-1);
	}
	.tpg-chip--2 {
		background: var(--tpg-chip-2);
	}
	.tpg-chip--3 {
		background: var(--tpg-chip-3);
	}
	.tpg-chip--4 {
		background: var(--tpg-chip-4);
	}
	.tpg-chip--5 {
		background: var(--tpg-chip-5);
	}
	.tpg-chip--muted {
		background: var(--text-faint);
	}
	.tpg-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
		letter-spacing: -0.01em;
		white-space: nowrap;
	}
	.tpg-name--link {
		text-decoration: none;
		border-radius: 2px;
	}
	.tpg-name--link:hover {
		text-decoration: underline;
		text-decoration-color: var(--text-faint);
	}
	.tpg-name--link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.tpg-path {
		font-size: var(--text-caption);
		color: var(--text-faint);
		overflow-wrap: anywhere;
		white-space: normal;
		min-width: 0;
	}
	.tpg-fill {
		flex: 1 1 auto;
	}
	.tpg-counts {
		flex-shrink: 0;
		font-size: var(--text-caption);
		color: var(--text-3);
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}

	.tpg-table {
		--edge: 12px;
		--aug-bl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		background: var(--surface-card);
		overflow: hidden;
		box-shadow:
			inset 0 1px 0 var(--surface-shine),
			var(--shadow-sm);
	}
	.tpg-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
	}

	.tpg-row {
		display: flex;
		flex-direction: column;
		border-bottom: 1px solid var(--border-subtle);
		border-left: 2px solid var(--border-subtle);
		transition: background var(--dur-1) var(--ease-snap);
	}
	.tpg-row:last-child {
		border-bottom: none;
	}
	.tpg-row:hover {
		background: var(--surface-2);
	}
	.tpg-row:hover .tpg-tool-backing,
	.tpg-row--expanded .tpg-tool-backing,
	.tpg-row:hover .tpg-failed-backing,
	.tpg-row--expanded .tpg-failed-backing {
		background: var(--surface-2);
	}
	.tpg-row--file {
		border-left-color: var(--role-tool-border);
	}
	.tpg-row--failed {
		border-left-color: var(--status-danger-border);
	}
	.tpg-row--expanded {
		background: var(--surface-2);
	}
	.tpg-row__head {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
		min-width: 0;
		box-sizing: border-box;
		padding: 4px 12px 4px 14px;
		min-height: var(--control-h);
		border-bottom: 1px solid transparent;
	}
	.tpg-row--expanded .tpg-row__head {
		border-bottom-color: var(--border-subtle);
	}
	.tpg-row__meta {
		display: flex;
		align-items: center;
		gap: 10px;
		flex: 0 0 auto;
		flex-wrap: wrap;
	}
	.tpg-row__disclose {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 10px;
		flex: 1 1 auto;
		min-width: 0;
		align-self: stretch;
		background: none;
		border: none;
		border-radius: 0;
		padding: 0;
		margin: 0;
		text-align: left;
		cursor: pointer;
		color: inherit;
		font: inherit;
	}
	.tpg-row__disclose:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}
	.tpg-row__body {
		display: grid;
		grid-template-rows: 0fr;
		min-width: 0;
		transition: grid-template-rows var(--dur-2) var(--ease-io);
	}
	.tpg-row__body--open {
		grid-template-rows: 1fr;
	}
	.tpg-row__body-clip {
		overflow: hidden;
		min-width: 0;
	}
	.tpg-row__body-clip > .tpg-section + .tpg-section {
		margin-top: 8px;
	}
	.tpg-row__body-clip > .tpg-pre,
	.tpg-row__body-clip > .tpg-section {
		margin: 8px 12px 8px 14px;
	}

	.tpg-tool-backing {
		--tpg-acc-fg: var(--text-strong);
		--tpg-acc-bg: var(--surface-2);
		--tpg-acc-border: var(--border-default);
		display: inline-flex;
		min-width: 0;
		overflow-wrap: anywhere;
		background: var(--surface-card);
	}
	.tpg-tool-backing.tpg-tool--file {
		--tpg-acc-fg: var(--role-tool-fg);
		--tpg-acc-bg: var(--role-tool-bg);
		--tpg-acc-border: var(--role-tool-border);
	}
	.tpg-tool-backing.tpg-tool--bash {
		--tpg-acc-fg: var(--status-info-fg);
		--tpg-acc-bg: var(--status-info-bg);
		--tpg-acc-border: var(--status-info-border);
	}
	.tpg-tool-backing.tpg-tool--task {
		--tpg-acc-fg: var(--data-teal-fg);
		--tpg-acc-bg: var(--data-teal-bg);
		--tpg-acc-border: var(--data-teal-border);
	}
	.tpg-tool {
		--aug-tl: 4px;
		--aug-br: 4px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--tpg-acc-border);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: var(--text-label);
		color: var(--tpg-acc-fg);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		overflow-wrap: anywhere;
		padding: 2px 7px;
		background: linear-gradient(0deg, var(--tpg-acc-bg), var(--tpg-acc-bg)), var(--surface-card);
	}
	.tpg-failed-backing {
		display: inline-flex;
		background: var(--surface-card);
	}
	.tpg-failed {
		--aug-tl: 4px;
		--aug-br: 4px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-danger-border);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: var(--text-label);
		color: var(--status-danger-fg);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		white-space: nowrap;
		padding: 2px 7px;
		background:
			linear-gradient(0deg, var(--status-danger-bg), var(--status-danger-bg)), var(--surface-card);
	}
	.tpg-ts,
	.tpg-meta {
		font-size: var(--text-caption);
		color: var(--text-3);
		white-space: nowrap;
	}
	.tpg-meta {
		font-variant-numeric: tabular-nums;
	}
	.tpg-meta--hash {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
	}
	.tpg-file {
		font-size: var(--text-caption);
		color: var(--text-3);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.tpg-link {
		font-size: var(--text-caption);
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
		white-space: nowrap;
	}
	.tpg-link:hover {
		color: var(--text-strong);
		text-decoration-color: var(--status-info-fg);
	}
	.tpg-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}

	.tpg-section {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.tpg-section__head {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.tpg-section__label {
		font-size: var(--text-label);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.tpg-pre {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-label);
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
		overflow-wrap: anywhere;
		line-height: 1.55;
		margin: 0;
		padding: calc(var(--edge-sm) + var(--space-1));
		background: var(--surface-bg);
		max-height: 480px;
		overflow-y: auto;
	}
	.tpg-pre--error {
		border-left: 2px solid var(--status-danger-border);
	}
</style>
