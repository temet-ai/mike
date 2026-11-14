<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { RefreshCw, ScanSearch } from '@lucide/svelte';
	import { ApiError } from '$lib/api';
	import { getCodeFindings, type CodeFindingsResponse } from '$lib/api/codeFindings';
	import RuleOutcomeTable from '$lib/components/code-findings/RuleOutcomeTable.svelte';
	import RuleCatalogueTable from '$lib/components/code-findings/RuleCatalogueTable.svelte';
	import CheckoutRulesTable from '$lib/components/code-findings/CheckoutRulesTable.svelte';
	import { formatMicros, totalRules } from '$lib/components/code-findings/codeFindingsFormat';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import OverviewBand from '$lib/components/shared/OverviewBand.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Select } from '$lib/components/ui/select';
	import { formatRelative } from '$lib/time';

	const WINDOWS = [
		{ value: '24', label: '24 hours' },
		{ value: '168', label: '7 days' },
		{ value: '720', label: '30 days' }
	];
	const DEFAULT_HOURS = '168';

	function initialWindow(): string {
		const raw = page.url.searchParams.get('hours');
		return raw !== null && WINDOWS.some((w) => w.value === raw) ? raw : DEFAULT_HOURS;
	}

	let windowHours = $state(initialWindow());
	let data = $state<CodeFindingsResponse | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let loadedAt = $state(Date.now());
	let statusAnnouncement = $state('');

	const rules = $derived(data?.rules ?? []);
	const totals = $derived(totalRules(rules));
	const erroring = $derived((data?.project_rules ?? []).filter((entry) => entry.error).length);

	async function load(): Promise<void> {
		loading = true;
		error = null;
		try {
			const result = await getCodeFindings({ hours: Number(windowHours), limit: 1 });
			data = result;
			loadedAt = Date.now();
			statusAnnouncement = `Loaded ${result.rules.length} rules that fired.`;
		} catch (err) {
			error = err instanceof ApiError ? err.message : String(err);
			statusAnnouncement = "Couldn't load code findings.";
		} finally {
			loading = false;
		}
	}

	function onWindowChange(value: string): void {
		windowHours = value;
		void goto(`${resolve('/code-findings')}?hours=${windowHours}`, {
			replace: true,
			reset: false
		});
		void load();
	}

	onMount(() => {
		void load();
	});

	function onOff(value: boolean): string {
		return value ? 'on' : 'off';
	}

	const facts = $derived([
		{ label: 'Scanned', value: data ? data.summary.scanned : '—' },
		{ label: 'Skipped', value: data ? data.summary.skipped : '—' },
		{ label: 'Failed', value: data ? data.summary.failed : '—' },
		{ label: 'Median scan', value: data ? formatMicros(data.summary.median_us) : '—' },
		{ label: 'Slowest scan', value: data ? formatMicros(data.summary.max_us) : '—' },
		{
			label: 'Last scan',
			value: data ? formatRelative(data.summary.last_scan_at, loadedAt) : '—'
		}
	]);
</script>

<svelte:head>
	<title>Code findings — mike</title>
</svelte:head>

<section class="cf-page" data-slot="code-findings-page">
	<PageIntro
		title="Code findings"
		level="h1"
		description="Rules that flag code a session just wrote, and whether sessions fix what they are told."
	>
		{#snippet actions()}
			<Select
				label="Window"
				options={WINDOWS}
				bind:value={windowHours}
				onchange={(event) => onWindowChange(event.currentTarget.value)}
				disabled={loading}
			/>
			<Button variant="ghost" size="sm" onclick={() => void load()} disabled={loading}>
				<RefreshCw size={12} aria-hidden="true" />
				Reload
			</Button>
		{/snippet}
	</PageIntro>

	<span class="sr-only" role="status" aria-live="polite">{statusAnnouncement}</span>

	{#if error}
		<LoadErrorBanner
			title="Couldn't load code findings"
			detail={error}
			hint="The inspect API at :9989 may not be reachable."
			onRetry={() => void load()}
		/>
	{:else}
		<OverviewBand label="Scan health" {facts}>
			{#if data && !data.scanning}
				Scanning is off. Nothing is recorded.
			{:else if data}
				Scanning {onOff(data.scanning)}, telling sessions {onOff(data.delivering)}, pushing
				{onOff(data.pushing)}.
			{/if}
			{#if data}
				<a class="cf-link" href={resolve('/settings')}>Settings</a>
			{/if}
		</OverviewBand>

		<div class="edge mech-group" data-slot="code-findings-bands">
			<SectionCard
				title="Rules that fired"
				titleIcon={ScanSearch}
				collapsible
				bordered={false}
				sectionKey="code-findings:rules"
				titleTooltip="Rules with at least one finding in the window, most found first."
			>
				{#snippet headerActions()}
					{#if data}
						<span
							class="font-mono text-[length:var(--text-caption)] text-[var(--text-3)] whitespace-nowrap"
							>{rules.length} rules · {totals.found.toLocaleString('en-US')} found</span
						>
					{/if}
				{/snippet}
				{#if loading && !data}
					<div class="cf-skeleton" aria-hidden="true">
						<div class="cf-skel-row"></div>
						<div class="cf-skel-row cf-skel-row--short"></div>
					</div>
				{:else if rules.length > 0}
					<RuleOutcomeTable {rules} catalogue={data?.catalogue ?? null} />
				{:else if data && !data.scanning}
					<p class="cf-empty">
						Scanning is off. <a class="cf-link" href={resolve('/settings')}>Settings</a>
					</p>
				{:else if data}
					<p class="cf-empty">No rule fired in this window.</p>
				{/if}
			</SectionCard>

			<SectionCard
				title="Rule catalogue"
				titleIcon={ScanSearch}
				collapsible
				bordered={false}
				sectionKey="code-findings:catalogue"
				defaultCollapsed
				titleTooltip="Every rule mike runs. Built-in rules ship with mike; project rules come from a checkout."
			>
				{#snippet headerActions()}
					{#if data?.catalogue}
						<span
							class="font-mono text-[length:var(--text-caption)] text-[var(--text-3)] whitespace-nowrap"
							>{data.catalogue.length} rules · version {data.rules_version ?? '—'}</span
						>
					{/if}
				{/snippet}
				{#if data?.catalogue}
					<div class="cf-catalogue">
						<p class="cf-lede">Grammars: {data.grammars.join(', ')}.</p>
						<RuleCatalogueTable catalogue={data.catalogue} />
					</div>
				{:else if data}
					<p class="cf-empty">The rule catalogue is not available.</p>
				{/if}
			</SectionCard>

			<SectionCard
				title="Checkouts and project rules"
				titleIcon={ScanSearch}
				collapsible
				bordered={false}
				sectionKey="code-findings:checkouts"
				defaultCollapsed
				titleTooltip="Checkouts the scanner watches and the rule files each one adds."
			>
				{#snippet headerActions()}
					{#if data}
						<span
							class="font-mono text-[length:var(--text-caption)] text-[var(--text-3)] whitespace-nowrap"
							>{data.watched_checkouts.length} watched{#if erroring > 0}&nbsp;<span
									class="cf-meta-bad">· {erroring} with errors</span
								>{/if}</span
						>
					{/if}
				{/snippet}
				{#if data && data.watched_checkouts.length === 0 && data.project_rules.length === 0}
					<p class="cf-empty">No checkout is watched yet.</p>
				{:else if data}
					<CheckoutRulesTable watched={data.watched_checkouts} projectRules={data.project_rules} />
				{/if}
			</SectionCard>
		</div>
	{/if}
</section>

<style>
	.cf-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		width: 100%;
		max-width: 1200px;
		margin: 0 auto;
		padding: var(--space-4);
		flex: 1 1 auto;
		min-width: 0;
		min-height: 0;
		overflow-y: auto;
	}
	.cf-meta-bad {
		color: var(--status-danger-fg);
	}
	.cf-empty,
	.cf-lede {
		margin: 0;
		color: var(--text-3);
	}
	.cf-lede {
		color: var(--text-2);
	}
	.cf-catalogue {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.cf-link {
		color: var(--interactive-active-fg);
	}
	.cf-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.cf-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.cf-skel-row--short {
		width: 60%;
	}
</style>
