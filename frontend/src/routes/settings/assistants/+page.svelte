<script lang="ts">
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';

	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import WorkerCard from '$lib/components/settings/WorkerCard.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import { ApiError } from '$lib/api';
	import { getAssistantHealth } from '$lib/api/assistant';
	import type { AssistantHealth } from '$lib/api/assistant';
	import { getAnthropicIntegration, getAnthropicCliStatus } from '$lib/api/integrations';
	import type { AnthropicIntegrationStatus, AnthropicCliStatus } from '$lib/api/integrations';
	import { USAGE_MODELS_DAYS, getModelUsage } from '$lib/api/usage';
	import { getFeatures, getSettings, patchFeature, patchSetting } from '$lib/api/settings';
	import type { FeatureDto, SettingMeta, SettingValue } from '$lib/api/settings';
	import { buildClaudeModelOptions } from '$lib/claudeModels';
	import type { WorkerModel } from '$lib/components/settings/WorkerCard.svelte';
	import type { AssistantsPageData } from './+page';

	const { data }: { data: AssistantsPageData } = $props();

	let observedClaudeModelIds = $state<string[]>([]);
	const claudeModels = $derived(buildClaudeModelOptions(observedClaudeModelIds));
	const codexModels: WorkerModel[] = [
		{ id: 'gpt-6-astra', description: 'Frontier Codex model for the hardest judgment work.' },
		{ id: 'gpt-5.6-sol', description: 'Older coding model for complex work.' },
		{ id: 'gpt-5.6-terra', description: 'Older balanced model for straightforward work.' },
		{ id: 'gpt-5.6-luna', description: 'Older fast and efficient model.' }
	];

	let features = $state<FeatureDto[]>([]);
	let loadError = $state<string | null>(null);
	let settings = $state<SettingMeta[]>([]);
	let settingsLoadError = $state<string | null>(null);
	let health = $state<AssistantHealth | null>(null);
	let healthError = $state<string | null>(null);
	let anthropicMode = $state<AnthropicIntegrationStatus | null>(null);
	let anthropicCli = $state<AnthropicCliStatus | null>(null);
	let rowLoading = $state<Record<string, boolean>>({});
	let rowError = $state<Record<string, string | null>>({});
	let settingRowLoading = $state<Record<string, boolean>>({});
	let settingRowError = $state<Record<string, string | null>>({});

	$effect(() => {
		features = data.features ?? [];
		loadError = data.loadError;
		settings = data.settings ?? [];
		settingsLoadError = data.settingsLoadError;
		health = data.health;
		healthError = data.healthError;
		observedClaudeModelIds = data.observedClaudeModelIds;
	});

	onMount(() => {
		if (data.loadError !== null) void loadFeatures();
		if (data.settingsLoadError !== null) void loadSettings();
		if (data.healthError !== null) void loadHealth();
		void loadAnthropicAccessDetail();
		const a = onApiCacheUpdate(
			'/api/settings/features',
			() => {
				void loadFeatures();
			},
			{ prefix: true }
		);
		const b = onApiCacheUpdate(
			'/api/settings',
			() => {
				void loadSettings();
			},
			{ prefix: true }
		);
		const c = onApiCacheUpdate(
			'/api/usage/models',
			() => {
				void loadClaudeModelIds();
			},
			{ prefix: true }
		);
		const d = onApiCacheUpdate(
			'/api/settings/assistant/anthropic',
			() => {
				void loadAnthropicAccessDetail();
			},
			{ prefix: true }
		);
		return () => {
			a();
			b();
			c();
			d();
		};
	});

	async function loadAnthropicAccessDetail(): Promise<void> {
		try {
			anthropicMode = await getAnthropicIntegration();
		} catch {
			anthropicMode = null;
		}
		if (anthropicMode?.mode !== 'claude_code_login') {
			anthropicCli = null;
			return;
		}
		try {
			anthropicCli = await getAnthropicCliStatus();
		} catch {
			anthropicCli = null;
		}
	}

	async function loadClaudeModelIds(): Promise<void> {
		try {
			const result = await getModelUsage(USAGE_MODELS_DAYS);
			observedClaudeModelIds = [
				...new Set(
					result.models.map((m) => m.model).filter((m): m is string => m !== null && m !== '')
				)
			];
		} catch {
			// The maintained catalogue in $lib/claudeModels.ts is still shown; a failed
			// refresh here only means newly-observed models stay unlisted until retried.
		}
	}

	async function loadFeatures(): Promise<void> {
		loadError = null;
		try {
			features = await getFeatures();
		} catch (err) {
			loadError = err instanceof ApiError ? err.message : String(err);
		}
	}
	async function loadSettings(): Promise<void> {
		settingsLoadError = null;
		try {
			const r = await getSettings();
			settings = r.settings;
		} catch (err) {
			settingsLoadError = err instanceof ApiError ? err.message : String(err);
		}
	}
	async function loadHealth(): Promise<void> {
		healthError = null;
		try {
			health = await getAssistantHealth();
		} catch (err) {
			healthError = err instanceof ApiError ? err.message : String(err);
		}
	}

	async function handleFeatureToggle(key: string, newValue: boolean): Promise<void> {
		const previous = features.find((f) => f.key === key)?.enabled ?? true;
		features = features.map((f) => (f.key === key ? { ...f, enabled: newValue } : f));
		rowLoading = { ...rowLoading, [key]: true };
		rowError = { ...rowError, [key]: null };
		try {
			const saved = await patchFeature(key, newValue);
			features = features.map((f) => (f.key === key ? { ...f, ...saved } : f));
		} catch (err) {
			features = features.map((f) => (f.key === key ? { ...f, enabled: previous } : f));
			rowError = { ...rowError, [key]: err instanceof ApiError ? err.message : String(err) };
		} finally {
			rowLoading = { ...rowLoading, [key]: false };
		}
	}
	async function handleSettingSave(key: string, value: SettingValue): Promise<void> {
		const previous = settings.find((s) => s.key === key);
		if (!previous) return;
		if (
			previous.kind === 'bool' ||
			key === 'prompt_assistant.model' ||
			key === 'prompt_assistant.codex_model'
		)
			settings = settings.map((s) => (s.key === key ? { ...s, value } : s));
		settingRowLoading = { ...settingRowLoading, [key]: true };
		settingRowError = { ...settingRowError, [key]: null };
		try {
			const saved = await patchSetting(key, value);
			settings = settings.map((s) => (s.key === key ? { ...s, ...saved } : s));
		} catch (err) {
			if (
				previous.kind === 'bool' ||
				key === 'prompt_assistant.model' ||
				key === 'prompt_assistant.codex_model'
			)
				settings = settings.map((s) => (s.key === key ? { ...s, value: previous.value } : s));
			settingRowError = {
				...settingRowError,
				[key]: err instanceof ApiError ? err.message : String(err)
			};
		} finally {
			settingRowLoading = { ...settingRowLoading, [key]: false };
		}
	}

	const byKey = $derived(new Map(settings.map((s) => [s.key, s])));
	const selectedProvider = $derived(
		settingStr('assistant.provider') === 'codex' ? 'codex' : 'anthropic'
	);
	const assistantModels = $derived(selectedProvider === 'codex' ? codexModels : claudeModels);
	const assistantModelKey = $derived(
		selectedProvider === 'codex' ? 'prompt_assistant.codex_model' : 'prompt_assistant.model'
	);
	function featureEnabled(key: string): boolean {
		return features.find((f) => f.key === key)?.enabled ?? true;
	}
	function settingStr(key: string): string {
		const s = byKey.get(key);
		return typeof s?.value === 'string' ? s.value : '';
	}
	function appliesFor(key: string): string | undefined {
		return features.find((f) => f.key === key)?.applies;
	}

	const spendReadout = $derived(
		health !== null && selectedProvider === 'anthropic'
			? `Today: $${health.spent_today_usd.toFixed(2)} of $${health.daily_budget_usd.toFixed(2)}`
			: null
	);

	const anthropicAccessOk = $derived.by(() => {
		if (selectedProvider === 'codex') return health?.credentials_available ?? true;
		if (health === null) return true;
		if (health.auth_mode === 'api_key') return health.credentials_available;
		if (anthropicCli !== null) return anthropicCli.logged_in;
		return true;
	});
	const anthropicAccessLabel = $derived.by(() => {
		if (selectedProvider === 'codex') return 'Existing ChatGPT login through Codex';
		if (anthropicMode?.mode === 'api_key') {
			return anthropicMode.configured
				? `API key ${anthropicMode.masked_key ?? 'configured'}`
				: 'API key mode, but no key is saved';
		}
		if (anthropicMode?.mode === 'claude_code_login') {
			if (anthropicCli === null) return 'Claude Code login on this machine';
			if (anthropicCli.logged_in) return `Signed in as ${anthropicCli.email ?? 'unknown account'}`;
			return 'Not signed in';
		}
		if (health === null) return null;
		if (health.auth_mode === 'api_key') {
			return health.credentials_available
				? 'API key configured'
				: 'API key mode, but no key is saved';
		}
		return 'Claude Code login on this machine';
	});
</script>

<svelte:head><title>Assistants — Settings — mike</title></svelte:head>

<SettingsPage
	title="Assistants"
	current="/settings/assistants"
	loadErrors={[
		loadError === null ? null : `Features: ${loadError}`,
		settingsLoadError === null ? null : `Settings: ${settingsLoadError}`
	]}
	onRetry={() => {
		void loadFeatures();
		void loadSettings();
	}}
>
	{#snippet lede()}
		Assistants you talk to from inside mike. They run only when you ask them something.
	{/snippet}

	<div class="st-env" data-augmented-ui="tl-clip br-clip border">
		<div class="st-env-status" aria-live="polite">
			<span
				class="st-env-dot"
				class:st-env-dot--ok={anthropicAccessLabel === null || anthropicAccessOk}
				class:st-env-dot--warn={anthropicAccessLabel !== null && !anthropicAccessOk}
				aria-hidden="true"
			></span>
			<span class="st-env-label"
				>{selectedProvider === 'codex' ? 'Codex' : 'Anthropic'} access for the prompt assistant: {#if anthropicAccessLabel === null}<span
						class="st-env-pending">checking</span
					>{:else if anthropicAccessOk}<span class="st-env-ok">{anthropicAccessLabel}</span
					>{:else}<span class="st-env-warn">{anthropicAccessLabel}</span>{/if}
				— <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- resolve()'d route with a static in-page anchor appended; the rule only recognizes a bare resolve() call as an href expression -->
				<a href={resolve('/settings/integrations') + `#${selectedProvider}`}
					>change in Integrations</a
				></span
			>
		</div>
		{#if healthError !== null}
			<p class="st-env-note st-env-note--error">Could not reach the assistant: {healthError}</p>
		{:else if spendReadout !== null}
			<p class="st-env-note">{spendReadout}</p>
		{/if}
	</div>

	<WorkerCard
		name="prompt_assistant"
		label="Prompt assistant — rewords prompts in the filter workbench"
		description="The chat under Your edits in the filter workbench. Turning it off hides the chat; your edits stay."
		applies={appliesFor('prompt_assistant')}
		enabled={featureEnabled('prompt_assistant')}
		onToggle={(v) => void handleFeatureToggle('prompt_assistant', v)}
		error={rowError['prompt_assistant'] ?? null}
		accent="info"
		models={assistantModels}
		selectedModelId={settingStr(assistantModelKey)}
		onSelectModel={(id) => void handleSettingSave(assistantModelKey, id)}
	>
		<p class="st-running-on">
			Running on <strong>{settingStr(assistantModelKey)}</strong> through {selectedProvider ===
			'codex'
				? 'Codex'
				: 'Anthropic'}
		</p>
		{@render cfg(['prompt_assistant.instructions', 'prompt_assistant.effort'])}
		<div class="st-subsec-head">What it can read</div>
		{@render cfg([
			'prompt_assistant.read_prompt',
			'prompt_assistant.read_edits',
			'prompt_assistant.read_project_instructions',
			'prompt_assistant.read_versions',
			'prompt_assistant.read_memory'
		])}
		<div class="st-subsec-head">What it can change</div>
		{@render cfg(['prompt_assistant.may_apply'])}
		<div class="st-subsec-head">Limits</div>
		{@render cfg([
			'prompt_assistant.capture',
			'prompt_assistant.max_turns',
			'prompt_assistant.daily_budget_usd',
			'prompt_assistant.keep_days'
		])}
	</WorkerCard>
</SettingsPage>

{#snippet cfg(keys: string[])}
	<div class="st-cfg-list">
		{#each keys as key (key)}
			{@const s = byKey.get(key)}
			{#if s}
				<SettingRow
					setting={s}
					loading={settingRowLoading[key] ?? false}
					error={settingRowError[key] ?? null}
					onSave={(k, v) => void handleSettingSave(k, v)}
				/>
			{/if}
		{/each}
	</div>
{/snippet}

<style>
	.st-cfg-list {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		overflow: hidden;
		background: var(--surface-card);
	}
	.st-env {
		--edge: 8px;
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
		gap: 6px;
		padding: calc(var(--edge) + 4px) calc(var(--edge) + 6px);
		background: var(--surface-2);
	}
	.st-env-status {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}
	.st-env-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--text-3);
		flex-shrink: 0;
	}
	.st-env-dot--ok {
		background: var(--status-success-fg);
	}
	.st-env-dot--warn {
		background: var(--status-warning-fg);
	}
	.st-env-label {
		font-size: 12px;
		color: var(--text-2);
		flex: 1 1 auto;
		min-width: 0;
		overflow-wrap: break-word;
	}
	.st-env-label a {
		color: inherit;
		text-decoration: underline;
	}
	.st-env-label a:hover {
		color: var(--text-strong);
	}
	.st-env-ok {
		color: var(--status-success-fg);
	}
	.st-env-warn {
		color: var(--status-warning-fg);
	}
	.st-env-pending {
		color: var(--text-3);
	}
	.st-env-note {
		margin: 0;
		font-size: 11px;
		color: var(--text-3);
	}
	.st-env-note--error {
		color: var(--status-danger-fg);
	}
	.st-subsec-head {
		font-family: var(--font-display);
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		padding-top: 8px;
		border-top: 1px solid var(--border-subtle);
	}

	.st-running-on {
		margin: 0;
		font-size: 12px;
		color: var(--text-2);
	}
	.st-running-on strong {
		color: var(--text-strong);
	}
</style>
