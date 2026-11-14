<script lang="ts">
	import { RefreshCw, ScanSearch } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import InstrumentReadout from '$lib/components/shared/InstrumentReadout.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import { ApiError } from '$lib/api';
	import { getCodeFindings, type CodeFindingsResponse } from '$lib/api/codeFindings';
	import { createRetryBackoff } from '$lib/retryBackoff';
	import CodeFindingList from './CodeFindingList.svelte';
	import { COUNT_TOOLTIPS, plural } from './codeFindingsFormat';

	interface Props {
		sessionId: string;
		now: number;
		grouped?: boolean;
	}

	const INITIAL_LIMIT = 200;
	const MAX_LIMIT = 1000;
	const HOURS = 720;

	let { sessionId, now, grouped = false }: Props = $props();

	const sectionKey = $derived(`session:${sessionId}:code-findings-open`);

	let data = $state<CodeFindingsResponse | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let attemptedForSession = $state<string | null>(null);
	let limit = $state(INITIAL_LIMIT);
	let statusAnnouncement = $state('');

	const backoff = createRetryBackoff();

	const counts = $derived(data?.session_counts ?? null);
	const scanning = $derived(data?.scanning ?? false);

	const meta = $derived.by((): string => {
		if (error) return 'error';
		if (!data) return '';
		if (!scanning || !counts) return 'not scanned';
		return counts.open === 0 ? 'none open' : `${counts.open} open`;
	});

	const readouts = $derived(
		counts
			? [
					{
						label: 'Files scanned',
						value: counts.files_scanned,
						tip: COUNT_TOOLTIPS.files_scanned
					},
					{ label: 'Found', value: counts.found, tip: COUNT_TOOLTIPS.found },
					{ label: 'Told', value: counts.told, tip: COUNT_TOOLTIPS.told },
					{
						label: 'Fixed after told',
						value: counts.fixed_after_told,
						tip: COUNT_TOOLTIPS.fixed_after_told
					},
					{
						label: 'Fixed by other',
						value: counts.fixed_by_other,
						tip: COUNT_TOOLTIPS.fixed_by_other
					},
					{ label: 'Ignored', value: counts.ignored, tip: COUNT_TOOLTIPS.ignored },
					{ label: 'Open', value: counts.open, tip: COUNT_TOOLTIPS.open }
				]
			: []
	);

	async function load(id: string, requested: number): Promise<void> {
		backoff.cancel();
		loading = true;
		error = null;
		try {
			const result = await getCodeFindings({ session: id, hours: HOURS, limit: requested });
			if (id !== sessionId) return;
			data = result;
			limit = requested;
			backoff.succeeded();
			statusAnnouncement = `Loaded ${result.findings.length} code findings for this session.`;
		} catch (err) {
			if (id !== sessionId) return;
			error = err instanceof ApiError ? err.message : String(err);
			statusAnnouncement = "Couldn't load code findings.";
			backoff.failed(() => {
				if (id === sessionId) void load(id, requested);
			});
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const id = sessionId;
		if (attemptedForSession === id) return;
		attemptedForSession = id;
		data = null;
		limit = INITIAL_LIMIT;
		backoff.reset();
		void load(id, INITIAL_LIMIT);
	});

	$effect(() => backoff.cancel);

	function reload(): void {
		backoff.reset();
		void load(sessionId, limit);
	}

	function loadMore(): void {
		backoff.reset();
		void load(sessionId, Math.min(limit * 2, MAX_LIMIT));
	}
</script>

<SectionCard
	title="Code findings"
	titleIcon={ScanSearch}
	collapsible
	bordered={!grouped}
	{sectionKey}
	defaultCollapsed
	titleTooltip="ast-grep rules run on every file this session edits. A finding is code the edit added."
>
	{#snippet headerActions()}
		{#if meta}
			<span class="scf-meta" data-slot="session-code-findings-summary">{meta}</span>
		{/if}
	{/snippet}
	<span class="sr-only" role="status" aria-live="polite">{statusAnnouncement}</span>
	<div class="scf-body" data-slot="session-code-findings-panel">
		{#if loading && !data}
			<div class="scf-skeleton" aria-hidden="true">
				<div class="scf-skel-row"></div>
				<div class="scf-skel-row scf-skel-row--short"></div>
			</div>
		{/if}
		{#if error}
			<LoadErrorBanner
				title="Couldn't load code findings"
				detail={error}
				hint="The inspect API at :9989 may not be reachable."
				onRetry={reload}
			/>
		{:else if data}
			{#if !data.scanning}
				<p class="scf-note">
					Scanning is off, so nothing is recorded.
					<a class="scf-link" href={resolve('/settings')}>Settings</a>
				</p>
			{:else}
				{#if !data.delivering}
					<p class="scf-note">
						Findings are recorded, not sent to the session.
						<a class="scf-link" href={resolve('/settings')}>Settings</a>
					</p>
				{/if}
				{#if counts}
					<div class="scf-readouts">
						{#each readouts as readout (readout.label)}
							<Tooltip text={readout.tip}>
								{#snippet child({ props })}
									<button {...props} type="button" class="scf-readout">
										<InstrumentReadout
											label={readout.label}
											value={readout.value.toLocaleString('en-US')}
											accentToken="--text-3"
										/>
									</button>
								{/snippet}
							</Tooltip>
						{/each}
					</div>
					{#if counts.files_scanned === 0}
						<p class="scf-note">No edited file has been scanned yet.</p>
					{:else if data.findings.length === 0}
						<p class="scf-note">
							Nothing flagged in {counts.files_scanned.toLocaleString('en-US')}
							{plural(counts.files_scanned, 'file')}.
						</p>
					{/if}
				{/if}
				{#if data.findings.length > 0}
					<CodeFindingList findings={data.findings} {sessionId} {now} />
				{/if}
			{/if}
			<div class="scf-foot">
				<div class="scf-trunc">
					{#if data.scanning && data.findings_truncated}
						{#if limit >= MAX_LIMIT}
							<span class="scf-note">
								Showing the newest {MAX_LIMIT.toLocaleString('en-US')} findings. Counts above include
								all.
							</span>
						{:else}
							<span class="scf-note">Showing the newest {data.findings.length} findings.</span>
							<Button variant="outline" size="sm" onclick={loadMore} disabled={loading}>
								Load more
							</Button>
						{/if}
					{/if}
				</div>
				<div class="scf-actions">
					<Button variant="ghost" size="sm" onclick={reload} disabled={loading}>
						<RefreshCw size={12} aria-hidden="true" />
						Reload
					</Button>
					<a class="scf-link" href={resolve('/code-findings')}>All sessions</a>
				</div>
			</div>
		{/if}
	</div>
</SectionCard>

<style>
	.scf-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}
	.scf-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.scf-note {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.scf-readouts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-5);
	}
	.scf-readout {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: inherit;
		text-align: left;
		cursor: default;
	}
	.scf-readout:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.scf-foot {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}
	.scf-trunc,
	.scf-actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}
	.scf-actions {
		margin-inline-start: auto;
	}
	.scf-link {
		color: var(--interactive-active-fg);
		font-size: var(--text-sm);
		text-decoration: none;
	}
	.scf-link:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.scf-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.scf-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.scf-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.scf-skel-row--short {
		width: 60%;
	}
</style>
