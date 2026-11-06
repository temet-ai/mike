<script lang="ts">
	import type { Finding } from '$lib/types';
	import { resolve } from '$app/paths';
	import { LineChart } from '@lucide/svelte';
	import { CATEGORY_META, checkLabel } from '$lib/diagnostics';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props {
		findings: Finding[];
		loading: boolean;
		error: string | null;
		analysisHref: string;
		open?: boolean;
	}

	let { findings, loading, error, analysisHref, open: openProp = false }: Props = $props();

	// svelte-ignore state_referenced_locally
	let open = $state(openProp);

	const findingCount = $derived(findings.length);
</script>

<div class="sab-wrapper">
	<div class="band-hd-wrap">
		<button
			type="button"
			class="band-hd"
			aria-expanded={open}
			aria-controls="sab-body"
			onclick={() => {
				open = !open;
			}}
		>
			<DisclosureMarker {open} size={13} />
			<LineChart size={13} aria-hidden="true" />
			<span class="panel-hd">Flagged responses</span>
			<span class="band-hd__meta">
				{#if loading}
					loading…
				{:else if error}
					error loading findings
				{:else}
					{findingCount} {findingCount === 1 ? 'finding' : 'findings'}
				{/if}
			</span>
		</button>
	</div>

	{#if open}
		<div class="sab-body band-body" id="sab-body">
			<div class="sab-header">
				<h3 class="sab-hd">Diagnostic findings</h3>
				<p class="sab-desc">
					Flagged findings for this session. Each finding pairs a matched excerpt with full advice.
					Full text — no truncation.
				</p>
			</div>

			{#if loading}
				<div class="sab-loading">Loading findings…</div>
			{:else if error}
				<div class="sab-error">{error}</div>
			{:else if findings.length === 0}
				<div class="sab-empty">No findings for this session.</div>
			{:else}
				<div class="sab-findings">
					{#each findings as finding, i (i)}
						{@const meta = CATEGORY_META[finding.category]}
						<div class="sab-finding" data-motion="hover">
							<div class="sab-finding-body">
								<div class="sab-finding-header">
									<span class={`sab-category-badge ${meta.badgeClass}`}>{meta.label}</span>
									<span class="sab-analyzer">{checkLabel(finding.analyzer)}</span>
									<a
										href={resolve(
											`/sessions/${encodeURIComponent(finding.session_id)}/requests/${finding.request_id}` as `/sessions/${string}/requests/${string}`
										)}
										class="sab-req-link"
										aria-label="Go to request #{finding.request_id}"
									>
										req #{finding.request_id}
									</a>
								</div>

								<pre class="sab-matched-text">{finding.matched_text}</pre>

								<p class="sab-advice">{finding.advice}</p>

								{#if finding.other_hits > 0}
									<span class="sab-other-hits"
										>+{finding.other_hits} other similar hit{finding.other_hits === 1
											? ''
											: 's'}</span
									>
								{/if}
							</div>
						</div>
					{/each}
				</div>

				<div class="sab-footer">
					<a href={resolve(analysisHref as `/analysis?${string}`)} class="sab-analysis-link"
						>View all flagged responses →</a
					>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.sab-body {
		background: var(--surface-bg);
	}

	.sab-header {
		padding: 12px 0;
		border-bottom: 1px solid var(--border-subtle);
	}

	.sab-hd {
		margin: 0 0 3px;
		font-family: var(--font-display);
		font-size: 11px;
		font-weight: 800;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-2);
	}

	.sab-desc {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.sab-loading,
	.sab-empty {
		padding: 16px 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-3);
	}

	.sab-error {
		padding: 16px 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--status-danger-fg);
	}

	.sab-findings {
		display: flex;
		flex-direction: column;
	}

	.sab-finding {
		display: flex;
		align-items: stretch;
		border-bottom: 1px solid var(--border-subtle);
	}

	.sab-finding:last-child {
		border-bottom: none;
	}

	.sab-finding:hover {
		background: color-mix(in srgb, var(--surface-2) 35%, transparent 65%);
	}

	.sab-finding-body {
		flex: 1 1 auto;
		min-width: 0;
		padding: 12px 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.sab-finding-header {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.sab-category-badge {
		padding: 1px 7px;
		border-radius: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		flex-shrink: 0;
	}

	.sab-analyzer {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.sab-req-link {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--interactive-active-fg);
		text-decoration: none;
	}

	.sab-req-link:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.sab-req-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}

	.sab-matched-text {
		margin: 0;
		padding: 8px 10px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sab-advice {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-1);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sab-other-hits {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.sab-footer {
		padding: 10px 0;
		border-top: 1px solid var(--border-subtle);
	}

	.sab-analysis-link {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.sab-analysis-link:hover {
		color: var(--text-1);
	}

	.sab-analysis-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}
</style>
