<script lang="ts">
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';

	import { onMount } from 'svelte';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import OpenRouterIntegrationPanel from '$lib/components/settings/OpenRouterIntegrationPanel.svelte';
	import AnthropicIntegrationPanel from '$lib/components/settings/AnthropicIntegrationPanel.svelte';
	import CodexIntegrationPanel from '$lib/components/settings/CodexIntegrationPanel.svelte';
	import AssistantProviderSelector from '$lib/components/settings/AssistantProviderSelector.svelte';
	import ModelUsageTable from '$lib/components/settings/ModelUsageTable.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { Button } from '$lib/components/ui/button';
	import AccountStrip from '$lib/components/llmSpend/AccountStrip.svelte';
	import SpendByCallerChart from '$lib/components/llmSpend/SpendByCallerChart.svelte';
	import CallerTable from '$lib/components/llmSpend/CallerTable.svelte';
	import { ApiError } from '$lib/api';
	import { USAGE_MODELS_DAYS, getModelUsage, getOpenRouterUsage } from '$lib/api/usage';
	import type { ModelUsageRow, OpenRouterUsageDto } from '$lib/api/usage';
	import { listSteerers } from '$lib/api/steering';
	import type { SteererDto } from '$lib/types';
	import { getAnthropicIntegration, getCodexIntegration } from '$lib/api/integrations';
	import type { AnthropicIntegrationStatus, CodexIntegrationStatus } from '$lib/api/integrations';
	import { getLlmSpend, llmSpendSinceForDays, LLM_SPEND_WINDOW_OPTIONS } from '$lib/api/llmSpend';
	import type { LlmSpendResponse, LlmSpendWindowDays } from '$lib/api/llmSpend';
	import type { IntegrationsPageData } from './+page';
	import { LLM_SPEND_DEFAULT_WINDOW_DAYS } from './llmSpendWindow';

	const { data }: { data: IntegrationsPageData } = $props();

	let openrouterUsage = $state<OpenRouterUsageDto | null>(null);
	let openrouterError = $state<string | null>(null);
	let openrouterUnavailable = $state(false);
	let modelUsage = $state<ModelUsageRow[] | null>(null);
	let modelUsageError = $state<string | null>(null);
	let modelUsageUnavailable = $state(false);
	let steerers = $state<SteererDto[] | null>(null);
	let steerersError = $state<string | null>(null);
	let steerersUnavailable = $state(false);
	let anthropicStatus = $state<AnthropicIntegrationStatus | null>(null);
	let anthropicError = $state<string | null>(null);
	let anthropicUnavailable = $state(false);
	let codexStatus = $state<CodexIntegrationStatus | null>(null);
	let codexError = $state<string | null>(null);
	let codexUnavailable = $state(false);
	let provider = $state<'anthropic' | 'codex'>('anthropic');
	let spend = $state<LlmSpendResponse | null>(null);
	let spendError = $state<string | null>(null);
	let windowDays = $state<LlmSpendWindowDays>(LLM_SPEND_DEFAULT_WINDOW_DAYS);

	const openrouterConfigured = $derived(openrouterUsage?.configured === true);

	$effect(() => {
		openrouterUsage = data.openrouterUsage;
		openrouterError = data.openrouterError;
		openrouterUnavailable = data.openrouterUnavailable;
		modelUsage = data.modelUsage;
		modelUsageError = data.modelUsageError;
		modelUsageUnavailable = data.modelUsageUnavailable;
		steerers = data.steerers;
		steerersError = data.steerersError;
		steerersUnavailable = data.steerersUnavailable;
		anthropicStatus = data.anthropicStatus;
		anthropicError = data.anthropicError;
		anthropicUnavailable = data.anthropicUnavailable;
		codexStatus = data.codexStatus;
		codexError = data.codexError;
		codexUnavailable = data.codexUnavailable;
		provider = data.provider;
		spend = data.spend;
		spendError = data.spendError;
	});

	onMount(() => {
		if (data.openrouterError !== null && !data.openrouterUnavailable) void loadOpenRouterUsage();
		if (data.modelUsageError !== null && !data.modelUsageUnavailable) void loadModelUsage();
		if (data.steerersError !== null && !data.steerersUnavailable) void loadSteerers();
		if (data.anthropicError !== null && !data.anthropicUnavailable) void loadAnthropicStatus();
		if (data.codexError !== null && !data.codexUnavailable) void loadCodexStatus();
		if (data.spendError !== null) void loadSpend(windowDays);
		const a = onApiCacheUpdate(
			'/api/usage/openrouter',
			() => {
				void loadOpenRouterUsage();
			},
			{ prefix: true }
		);
		const b = onApiCacheUpdate(
			'/api/usage/models',
			() => {
				void loadModelUsage();
			},
			{ prefix: true }
		);
		const c = onApiCacheUpdate(
			'/api/steerers',
			() => {
				void loadSteerers();
			},
			{ prefix: true }
		);
		// No onApiCacheUpdate subscription for the Anthropic status path: getAnthropicIntegration
		// reads it 'passthrough' (never cached), so AnthropicIntegrationPanel's onStatusChanged
		// callback is the one and only refresh trigger after a mutation.
		return () => {
			a();
			b();
			c();
		};
	});

	function describe(err: unknown): { message: string; unavailable: boolean } {
		if (err instanceof ApiError) return { message: err.message, unavailable: err.status === 404 };
		return { message: err instanceof Error ? err.message : String(err), unavailable: false };
	}

	async function loadOpenRouterUsage(): Promise<void> {
		openrouterError = null;
		try {
			openrouterUsage = await getOpenRouterUsage();
			openrouterUnavailable = false;
		} catch (err) {
			const d = describe(err);
			openrouterError = d.message;
			openrouterUnavailable = d.unavailable;
		}
	}

	async function loadModelUsage(): Promise<void> {
		modelUsageError = null;
		try {
			const result = await getModelUsage(USAGE_MODELS_DAYS);
			modelUsage = result.models;
			modelUsageUnavailable = false;
		} catch (err) {
			const d = describe(err);
			modelUsageError = d.message;
			modelUsageUnavailable = d.unavailable;
		}
	}

	async function loadSteerers(): Promise<void> {
		steerersError = null;
		try {
			steerers = await listSteerers();
			steerersUnavailable = false;
		} catch (err) {
			const d = describe(err);
			steerersError = d.message;
			steerersUnavailable = d.unavailable;
		}
	}

	async function loadAnthropicStatus(): Promise<void> {
		anthropicError = null;
		try {
			anthropicStatus = await getAnthropicIntegration();
			anthropicUnavailable = false;
		} catch (err) {
			const d = describe(err);
			anthropicError = d.message;
			anthropicUnavailable = d.unavailable;
		}
	}

	async function loadCodexStatus(): Promise<void> {
		codexError = null;
		try {
			codexStatus = await getCodexIntegration();
			codexUnavailable = false;
		} catch (err) {
			const d = describe(err);
			codexError = d.message;
			codexUnavailable = d.unavailable;
		}
	}

	async function loadSpend(days: LlmSpendWindowDays): Promise<void> {
		spendError = null;
		try {
			spend = await getLlmSpend({ since: llmSpendSinceForDays(days) });
		} catch (err) {
			const d = describe(err);
			spendError = d.message;
		}
	}

	function selectWindow(days: LlmSpendWindowDays): void {
		if (days === windowDays) return;
		windowDays = days;
		void loadSpend(days);
	}

	function providerChanged(next: 'anthropic' | 'codex'): void {
		provider = next;
		if (codexStatus) codexStatus = { ...codexStatus, selected: next === 'codex' };
	}
</script>

<svelte:head><title>Integrations — Settings — mike</title></svelte:head>

<SettingsPage title="Integrations" current="/settings/integrations">
	{#snippet lede()}
		External services this app talks to, and the account behind each one.
	{/snippet}
	<AssistantProviderSelector {provider} onChanged={providerChanged} />

	<AnthropicIntegrationPanel
		status={anthropicStatus}
		statusError={anthropicError}
		statusUnavailable={anthropicUnavailable}
		onRetryStatus={() => void loadAnthropicStatus()}
		onStatusChanged={() => void loadAnthropicStatus()}
	/>

	<CodexIntegrationPanel
		status={codexStatus}
		error={codexError}
		unavailable={codexUnavailable}
		onRetry={() => void loadCodexStatus()}
	/>

	<OpenRouterIntegrationPanel
		usage={openrouterUsage}
		usageError={openrouterError}
		usageUnavailable={openrouterUnavailable}
		onRetryUsage={() => void loadOpenRouterUsage()}
		{steerers}
		{steerersError}
		{steerersUnavailable}
		onRetrySteerers={() => void loadSteerers()}
	/>

	{#if openrouterConfigured}
		<AccountStrip
			usage={openrouterUsage}
			today={spend?.today ?? null}
			todayError={spendError}
			onRetryToday={() => void loadSpend(windowDays)}
		/>

		<div class="st-window">
			<span class="st-window-label">Spend window</span>
			<div class="st-window-buttons" role="group" aria-label="Spend window">
				{#each LLM_SPEND_WINDOW_OPTIONS as days (days)}
					<Button
						type="button"
						variant={windowDays === days ? 'default' : 'outline'}
						size="sm"
						aria-pressed={windowDays === days}
						onclick={() => selectWindow(days)}
					>
						{days}d
					</Button>
				{/each}
			</div>
		</div>

		{#if spendError !== null}
			<LoadErrorBanner
				title="Could not load LLM spend"
				detail={spendError}
				onRetry={() => void loadSpend(windowDays)}
			/>
		{:else if spend === null}
			<p class="st-spend-loading" aria-busy="true">Loading LLM spend...</p>
		{:else if spend.by_caller === undefined || spend.by_day_caller === undefined}
			<p class="st-spend-unavailable">
				Per-caller spend breakdown is not available from this proxy build yet.
			</p>
		{:else}
			{@const byCaller = spend.by_caller ?? []}
			{@const byDayCaller = spend.by_day_caller ?? []}
			<div class="st-chart-card" data-augmented-ui="tl-clip br-clip border">
				<SpendByCallerChart callers={byCaller} {byDayCaller} />
			</div>
			<CallerTable callers={byCaller} totalCostUsd={spend.totals.cost_usd} />
		{/if}
	{/if}

	<ModelUsageTable
		rows={modelUsage}
		error={modelUsageError}
		unavailable={modelUsageUnavailable}
		days={USAGE_MODELS_DAYS}
		onRetry={() => void loadModelUsage()}
	/>
</SettingsPage>

<style>
	.st-window {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}
	.st-window-label {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}
	.st-window-buttons {
		display: flex;
		gap: 6px;
	}
	.st-spend-loading,
	.st-spend-unavailable {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
	}
	.st-spend-unavailable {
		font-style: italic;
	}
	.st-chart-card {
		--edge-tl: 8px;
		--edge-br: 10px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg) 0%,
			var(--data-teal-border) 35%,
			var(--border-default) 70%
		);
		padding: 14px 16px 16px;
		background: var(--surface-card);
	}
</style>
