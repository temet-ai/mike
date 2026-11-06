<script lang="ts">
	import { formatCount } from '$lib/time';
	import { estimateTokens, formatTokens } from '$lib/components/message-view/parseSystemPrompt';
	import { diffBlocks, type DiffRow } from '$lib/utils/blockDiff';

	interface Props {
		composition: string | null;
		observed: string | null;
		loading?: boolean;
		error?: string | null;
		leftLabel?: string;
		rightLabel?: string;
		splitPattern?: RegExp;
		loadingText?: string;
		errorPrefix?: string;
		emptyText?: string;
	}

	let {
		composition,
		observed,
		loading = false,
		error = null,
		leftLabel = 'Composition',
		rightLabel = 'Observed',
		splitPattern = /\n{2,}/,
		loadingText = 'Loading observed system prompt…',
		errorPrefix = 'Could not load observed prompt',
		emptyText = 'No composition or observed prompt to compare.'
	}: Props = $props();

	function splitBlocks(raw: string | null): string[] {
		if (raw === null || raw.length === 0) return [];
		return raw.split(splitPattern).filter((b) => b.length > 0);
	}

	const compositionBlocks = $derived(splitBlocks(composition));
	const observedBlocks = $derived(splitBlocks(observed));

	const rows = $derived(diffBlocks({ original: compositionBlocks, sent: observedBlocks }));

	const compositionChars = $derived(composition?.length ?? 0);
	const observedChars = $derived(observed?.length ?? 0);
	const charDelta = $derived(observedChars - compositionChars);
	const tokenDelta = $derived(estimateTokens(observedChars) - estimateTokens(compositionChars));

	const hasDrift = $derived(rows.some((row) => row.status !== 'unchanged') && rows.length > 0);

	function formatDelta(value: number): string {
		if (value === 0) return '0';
		const sign = value > 0 ? '+' : '−';
		return `${sign}${formatCount(Math.abs(value))}`;
	}

	function statusAnnotation(row: DiffRow): string {
		switch (row.status) {
			case 'unchanged':
				return 'unchanged';
			case 'dropped':
				return 'missing in observed';
			case 'appended':
				return 'extra in observed';
			case 'replaced':
				return 'drifted';
		}
	}

	function blockCharCount(text: string): number {
		return text.length;
	}
</script>

<div data-slot="prompt-composition-diff" class="flex flex-col gap-2">
	<header class="flex items-center justify-between gap-2 px-1">
		<span class="text-label font-mono tabular-nums text-[var(--text-2)]">
			{compositionBlocks.length} → {observedBlocks.length} block{observedBlocks.length === 1
				? ''
				: 's'} · {formatDelta(charDelta)} chars · ~{formatDelta(tokenDelta)} tokens
		</span>
		{#if hasDrift}
			<span
				class="inline-flex items-center gap-1 rounded-sm bg-[var(--status-warning-bg)] px-2 py-0.5 text-micro font-semibold uppercase tracking-kpi text-[var(--status-warning-fg)]"
			>
				<span class="block size-1.5 rounded-full bg-[var(--status-warning-fg)]" aria-hidden="true"
				></span>
				Drift detected
			</span>
		{:else}
			<span
				class="inline-flex items-center gap-1 rounded-sm bg-[var(--status-success-bg)] px-2 py-0.5 text-micro font-semibold uppercase tracking-kpi text-[var(--status-success-fg)]"
			>
				<span class="block size-1.5 rounded-full bg-[var(--status-success-fg)]" aria-hidden="true"
				></span>
				Matches {leftLabel.toLowerCase()}
			</span>
		{/if}
	</header>

	{#if error}
		<p class="text-sm italic text-[var(--status-danger-fg)]">
			{errorPrefix}: {error}
		</p>
	{:else if loading}
		<p class="text-sm italic text-[var(--text-3)]">{loadingText}</p>
	{:else if rows.length === 0}
		<p class="text-sm italic text-[var(--text-3)]">{emptyText}</p>
	{:else}
		<div
			role="region"
			aria-label="Prompt composition diff"
			class="sp-diff-grid"
			data-slot="prompt-composition-diff-grid"
		>
			<div class="sp-diff-col-head">{leftLabel}</div>
			<div class="sp-diff-col-head">{rightLabel}</div>

			{#each rows as row, i (i)}
				{@const hasLeft = row.status !== 'appended'}
				{@const hasRight = row.status !== 'dropped'}
				{@const leftText =
					row.status === 'unchanged'
						? row.text
						: row.status === 'replaced'
							? row.originalText
							: row.status === 'dropped'
								? row.text
								: ''}
				{@const rightText =
					row.status === 'unchanged'
						? row.text
						: row.status === 'replaced'
							? row.sentText
							: row.status === 'appended'
								? row.text
								: ''}
				{@const leftIndex = row.status === 'appended' ? null : row.originalIndex + 1}
				{@const rightIndex = row.status === 'dropped' ? null : row.sentIndex + 1}

				<div class="sp-diff-row" data-status={row.status}>
					<div
						class="sp-diff-cell"
						class:sp-diff-cell--dropped={row.status === 'dropped'}
						class:sp-diff-cell--replaced-left={row.status === 'replaced'}
						class:sp-diff-cell--placeholder={row.status === 'appended'}
						data-status={row.status}
						data-side="composition"
					>
						<span class="sr-only">{statusAnnotation(row)}</span>
						{#if hasLeft}
							<div class="sp-diff-cell__head">
								<span class="sp-diff-cell__marker" aria-hidden="true">
									{#if row.status === 'dropped'}−{:else if row.status === 'replaced'}~{:else}·{/if}
								</span>
								<span class="sp-diff-cell__index">#{leftIndex}</span>
							</div>
							<div class="sp-diff-cell__body">{leftText}</div>
							<div class="sp-diff-cell__meta">
								<span>{formatCount(blockCharCount(leftText))} chars</span>
								<span>{formatTokens(estimateTokens(blockCharCount(leftText)))}</span>
								<span class="sp-diff-cell__annotation">{statusAnnotation(row)}</span>
							</div>
						{:else}
							<div class="sp-diff-cell__placeholder">
								(extra in {rightLabel.toLowerCase()} — no {leftLabel.toLowerCase()} match)
							</div>
						{/if}
					</div>

					<div
						class="sp-diff-cell"
						class:sp-diff-cell--appended={row.status === 'appended'}
						class:sp-diff-cell--replaced-right={row.status === 'replaced'}
						class:sp-diff-cell--placeholder={row.status === 'dropped'}
						data-status={row.status}
						data-side="observed"
					>
						<span class="sr-only">{statusAnnotation(row)}</span>
						{#if hasRight}
							<div class="sp-diff-cell__head">
								<span class="sp-diff-cell__marker" aria-hidden="true">
									{#if row.status === 'appended'}+{:else if row.status === 'replaced'}~{:else}·{/if}
								</span>
								<span class="sp-diff-cell__index">#{rightIndex}</span>
							</div>
							<div class="sp-diff-cell__body">{rightText}</div>
							<div class="sp-diff-cell__meta">
								<span>{formatCount(blockCharCount(rightText))} chars</span>
								<span>{formatTokens(estimateTokens(blockCharCount(rightText)))}</span>
								<span class="sp-diff-cell__annotation">{statusAnnotation(row)}</span>
							</div>
						{:else}
							<div class="sp-diff-cell__placeholder">
								(missing in {rightLabel.toLowerCase()} — {leftLabel.toLowerCase()} expected)
							</div>
						{/if}
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}

	.sp-diff-grid {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 6px 8px;
		align-items: stretch;
	}

	.sp-diff-col-head {
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: var(--tracking-kpi);
		color: var(--text-3);
		font-weight: 600;
		padding: 4px 8px;
		border-bottom: 1px solid var(--border-subtle);
	}

	.sp-diff-cell {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
		background: var(--surface-2);
		color: var(--text-1);
		min-width: 0;
	}

	.sp-diff-cell--dropped,
	.sp-diff-cell--replaced-left {
		background: var(--status-danger-bg);
		border-color: var(--status-danger-border);
	}
	.sp-diff-cell--appended,
	.sp-diff-cell--replaced-right {
		background: var(--status-success-bg);
		border-color: var(--status-success-border);
	}
	.sp-diff-cell--placeholder {
		background: transparent;
		border-style: dashed;
		border-color: var(--border-subtle);
	}

	.sp-diff-row {
		display: contents;
	}
	.sp-diff-cell {
		transition:
			box-shadow 120ms ease,
			border-color 120ms ease;
	}
	:global(.sp-diff-row:hover > .sp-diff-cell) {
		box-shadow: inset 3px 0 0 var(--interactive-active-fg);
	}
	.sp-diff-cell:hover {
		box-shadow:
			inset 3px 0 0 var(--interactive-active-fg),
			0 0 0 1px var(--interactive-active-fg);
		border-color: var(--interactive-active-fg);
	}

	.sp-diff-cell__head {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: var(--text-label);
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}

	.sp-diff-cell__marker {
		font-family:
			'JetBrains Mono Variable', 'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace;
		font-weight: 700;
		width: 12px;
		text-align: center;
		line-height: 1;
	}
	.sp-diff-cell--dropped .sp-diff-cell__marker,
	.sp-diff-cell--replaced-left .sp-diff-cell__marker {
		color: var(--status-danger-fg);
	}
	.sp-diff-cell--appended .sp-diff-cell__marker,
	.sp-diff-cell--replaced-right .sp-diff-cell__marker {
		color: var(--status-success-fg);
	}

	.sp-diff-cell__index {
		font-family:
			'JetBrains Mono Variable', 'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: var(--text-label);
		color: var(--text-3);
	}

	.sp-diff-cell__body {
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
	}

	.sp-diff-cell__meta {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
		color: var(--text-3);
	}
	.sp-diff-cell__annotation {
		margin-left: auto;
		color: var(--text-2);
	}

	.sp-diff-cell__placeholder {
		font-size: var(--text-label);
		color: var(--text-faint);
		font-style: italic;
		padding: 8px 0;
	}
</style>
