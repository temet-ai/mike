<script lang="ts">
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import PageAlarms from '$lib/components/settings/PageAlarms.svelte';
	import GuardQuestionsPanel from '$lib/components/settings/GuardQuestionsPanel.svelte';
	import GuardCalibrationPanel from '$lib/components/settings/GuardCalibrationPanel.svelte';

	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import WorkerCard from '$lib/components/settings/WorkerCard.svelte';
	import ModelTable from '$lib/components/settings/ModelTable.svelte';
	import ModelPickerPopover from '$lib/components/settings/ModelPickerPopover.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Plus, TriangleAlert } from '@lucide/svelte';
	import { ApiError } from '$lib/api';
	import { getFeatures, getSettings, patchFeature, patchSetting } from '$lib/api/settings';
	import type { FeatureDto, SettingMeta, SettingValue } from '$lib/api/settings';
	import { getOpenRouterUsage } from '$lib/api/usage';
	import type { OpenRouterUsageDto } from '$lib/api/usage';
	import { listSteerers, patchSteerer, listModels, setAllSteererModels } from '$lib/api/steering';
	import type { SteererDto, ModelEntry } from '$lib/types';
	import type { SteeringPageData } from './+page';

	const { data }: { data: SteeringPageData } = $props();

	let features = $state<FeatureDto[]>([]);
	let loadError = $state<string | null>(null);
	let settings = $state<SettingMeta[]>([]);
	let settingsLoadError = $state<string | null>(null);
	let rowLoading = $state<Record<string, boolean>>({});
	let rowError = $state<Record<string, string | null>>({});
	let settingRowLoading = $state<Record<string, boolean>>({});
	let settingRowError = $state<Record<string, string | null>>({});
	let openrouterUsage = $state<OpenRouterUsageDto | null>(null);
	let openrouterUsageError = $state<string | null>(null);
	let steerers = $state<SteererDto[]>([]);
	let steerersLoadError = $state<string | null>(null);
	let fleetToggleLoading = $state<Record<number, boolean>>({});
	let fleetToggleError = $state<Record<number, string | null>>({});
	let fleetModelLoading = $state<Record<number, boolean>>({});
	let fleetModelError = $state<Record<number, string | null>>({});
	let bulkModelLoading = $state(false);
	let bulkModelError = $state<string | null>(null);
	let bulkModelResult = $state<string | null>(null);
	let models = $state<ModelEntry[]>([]);
	let modelsLoading = $state(false);
	let modelsLoadError = $state<string | null>(null);

	$effect(() => {
		features = data.features ?? [];
		loadError = data.loadError;
		settings = data.settings ?? [];
		settingsLoadError = data.settingsLoadError;
		steerers = data.steerers ?? [];
		steerersLoadError = data.steerersLoadError ?? null;
		models = data.models ?? [];
		modelsLoadError = data.modelsLoadError;
	});

	onMount(() => {
		if (data.loadError !== null) void loadFeatures();
		if (data.settingsLoadError !== null) void loadSettings();
		if (data.steerersLoadError !== null) void loadFleet();
		if (data.modelsLoadError !== null) void loadModels();
		void loadOpenRouterUsage();
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
			'/api/steerers',
			() => {
				void loadFleet();
			},
			{ prefix: true }
		);
		const d = onApiCacheUpdate(
			'/api/usage/openrouter',
			() => {
				void loadOpenRouterUsage();
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
	async function loadFleet(): Promise<void> {
		steerersLoadError = null;
		try {
			steerers = await listSteerers();
		} catch (err) {
			steerersLoadError = err instanceof ApiError ? err.message : String(err);
		}
	}
	async function loadModels(): Promise<void> {
		modelsLoadError = null;
		modelsLoading = true;
		try {
			const resp = await listModels();
			if (resp === null) {
				modelsLoadError = 'Failed to load models from OpenRouter.';
				models = [];
			} else {
				models = resp.models;
			}
		} catch (err) {
			modelsLoadError = err instanceof ApiError ? err.message : String(err);
			models = [];
		} finally {
			modelsLoading = false;
		}
	}
	async function loadOpenRouterUsage(): Promise<void> {
		openrouterUsageError = null;
		try {
			openrouterUsage = await getOpenRouterUsage();
		} catch (err) {
			openrouterUsageError = err instanceof ApiError ? err.message : String(err);
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
		if (previous.kind === 'bool')
			settings = settings.map((s) => (s.key === key ? { ...s, value } : s));
		settingRowLoading = { ...settingRowLoading, [key]: true };
		settingRowError = { ...settingRowError, [key]: null };
		try {
			const saved = await patchSetting(key, value);
			settings = settings.map((s) => (s.key === key ? { ...s, ...saved } : s));
		} catch (err) {
			if (previous.kind === 'bool')
				settings = settings.map((s) => (s.key === key ? { ...s, value: previous.value } : s));
			settingRowError = {
				...settingRowError,
				[key]: err instanceof ApiError ? err.message : String(err)
			};
		} finally {
			settingRowLoading = { ...settingRowLoading, [key]: false };
		}
	}
	async function toggleSteererEnabled(s: SteererDto): Promise<void> {
		const id = s.id;
		const newEnabled = !s.enabled;
		steerers = steerers.map((x) => (x.id === id ? { ...x, enabled: newEnabled } : x));
		fleetToggleLoading = { ...fleetToggleLoading, [id]: true };
		fleetToggleError = { ...fleetToggleError, [id]: null };
		try {
			const envelope = await patchSteerer(id, { enabled: newEnabled });
			steerers = steerers.map((x) => (x.id === id ? envelope.steerer : x));
		} catch (err) {
			steerers = steerers.map((x) => (x.id === id ? { ...x, enabled: s.enabled } : x));
			fleetToggleError = {
				...fleetToggleError,
				[id]: err instanceof ApiError ? err.message : String(err)
			};
		} finally {
			fleetToggleLoading = { ...fleetToggleLoading, [id]: false };
		}
	}

	function effectiveModel(s: SteererDto): string {
		if (s.effective_model) return s.effective_model;
		if (s.model) return s.model;
		return settingStr('steerer.model') || '—';
	}
	function isInherited(s: SteererDto): boolean {
		return s.model_inherited ?? s.model === '';
	}

	function modelHierarchyHint(s: SteererDto): string {
		const fleetDefault = settingStr('steerer.model') || '(no fleet default set)';
		if (isInherited(s)) {
			return `No model stored for this steerer — it inherits the fleet default, currently ${fleetDefault}.`;
		}
		return `Stored override: ${s.model || effectiveModel(s)}. The fleet default is ${fleetDefault}; this steerer ignores it.`;
	}

	async function changeSteererModel(s: SteererDto, newModel: string): Promise<void> {
		const id = s.id;
		const previous = s.model;
		steerers = steerers.map((x) => (x.id === id ? { ...x, model: newModel } : x));
		fleetModelLoading = { ...fleetModelLoading, [id]: true };
		fleetModelError = { ...fleetModelError, [id]: null };
		try {
			const envelope = await patchSteerer(id, { model: newModel });
			steerers = steerers.map((x) => (x.id === id ? envelope.steerer : x));
		} catch (err) {
			steerers = steerers.map((x) => (x.id === id ? { ...x, model: previous } : x));
			fleetModelError = {
				...fleetModelError,
				[id]: err instanceof ApiError ? err.message : String(err)
			};
		} finally {
			fleetModelLoading = { ...fleetModelLoading, [id]: false };
		}
	}

	async function handleBulkModel(model: string): Promise<void> {
		bulkModelLoading = true;
		bulkModelError = null;
		bulkModelResult = null;
		try {
			const resp = await setAllSteererModels(model);
			bulkModelResult = `Set ${resp.count} steerer${resp.count === 1 ? '' : 's'} to ${model === '' ? 'inherit the default' : model}.`;
			await loadFleet();
		} catch (err) {
			bulkModelError = err instanceof ApiError ? err.message : String(err);
		} finally {
			bulkModelLoading = false;
		}
	}

	function triggerLabel(tp: string | undefined): string {
		return tp === 'user_prompt' ? 'user prompt' : 'response';
	}
	function invocationLabel(inv: string | undefined): string {
		return inv === 'pattern' ? 'pattern pre-filter' : 'every turn';
	}

	const byKey = $derived(new Map(settings.map((s) => [s.key, s])));
	const featureByKey = $derived(new Map(features.map((f) => [f.key, f])));
	const steererFeature = $derived(featureByKey.get('steerer') ?? null);
	function settingStr(key: string): string {
		const s = byKey.get(key);
		return typeof s?.value === 'string' ? s.value : '';
	}
	function appliesFor(key: string): string | undefined {
		return features.find((f) => f.key === key)?.applies;
	}
	const openrouterConfigured = $derived(openrouterUsage?.configured === true);
	const steererGated = $derived(!openrouterConfigured);
	const gateHint = $derived(
		openrouterUsage === null && openrouterUsageError === null
			? 'Checking your OpenRouter key…'
			: 'Configure your OpenRouter key in Integrations to enable the steerer.'
	);
</script>

<svelte:head><title>Steering — Settings — mike</title></svelte:head>

<SettingsPage
	title="Steering"
	current="/settings/steering"
	loadErrors={[
		loadError === null ? null : `Features: ${loadError}`,
		settingsLoadError === null ? null : `Settings: ${settingsLoadError}`
	]}
	onRetry={() => {
		void loadFeatures();
		void loadSettings();
		if (modelsLoadError !== null) void loadModels();
	}}
>
	{#snippet lede()}
		The steerer<HelpHint slug="steerer" /> fleet<HelpHint slug="fleet" /> checks each turn of an enrolled
		session and nudges it back when it drifts. The judges are on the
		<a class="st-env-link" href={resolve('/settings/workers')}>Workers page</a>.
	{/snippet}

	<GuardQuestionsPanel />
	<GuardCalibrationPanel />

	<div class="st-env" data-augmented-ui="tl-clip br-clip border">
		<div class="st-env-status" aria-live="polite">
			<span class="st-env-dot" class:st-env-dot--ok={openrouterConfigured} aria-hidden="true"
			></span>
			<span class="st-env-label"
				>OpenRouter API key:
				{#if openrouterUsageError !== null}
					<span class="st-env-missing">could not check ({openrouterUsageError})</span>
				{:else if openrouterUsage === null}
					<span class="st-env-missing">checking…</span>
				{:else if openrouterConfigured}
					<span class="st-env-ok"
						>configured — <a href={resolve('/settings/integrations')} class="st-env-link"
							>manage in Integrations</a
						></span
					>
				{:else}
					<span class="st-env-missing"
						>not configured — <a href={resolve('/settings/integrations')} class="st-env-link"
							>set it up in Integrations</a
						></span
					>
				{/if}
			</span>
		</div>
	</div>

	{#if steererFeature !== null}
		<WorkerCard
			name="steerer"
			label="Steerer — acts on live sessions"
			description="Checks each turn of an enrolled session and nudges it back while it is still running. Needs an OpenRouter key."
			applies={appliesFor('steerer')}
			enabled={steererFeature.enabled}
			disabled={steererGated}
			onToggle={(v) => void handleFeatureToggle('steerer', v)}
			accent="danger"
		>
			{#if modelsLoadError !== null}
				<div class="st-load-error" role="alert">
					<p class="st-err-msg">Could not load models: {modelsLoadError}</p>
					<span class="st-retry-backing"
						><button
							type="button"
							class="st-retry"
							data-augmented-ui="tr-clip bl-clip border"
							onclick={() => void loadModels()}>Retry</button
						></span
					>
				</div>
			{/if}
			<div class="st-model-block">
				<span class="st-model-label">Which model judges each turn</span>
				<p class="st-model-why">
					Every turn of an enrolled session is sent to this model. It decides whether the session
					has drifted from what you asked for, and writes the nudge that gets injected back into the
					session. It runs through OpenRouter and is billed per call, so the input and output prices
					in the list are what each check costs you.
				</p>
				<ModelTable
					{models}
					currentValue={settingStr('steerer.model')}
					loading={modelsLoading}
					saving={settingRowLoading['steerer.model'] ?? false}
					disabled={steererGated}
					onSelect={(id) => void handleSettingSave('steerer.model', id)}
				/>
				{#if settingRowError['steerer.model']}
					<p class="st-model-err" role="alert">
						Could not save the model: {settingRowError['steerer.model']}
					</p>
				{/if}
			</div>
			{@render cfg([
				'steerer.request_timeout_secs',
				'steerer.quiet_period_secs',
				'steerer.quiescence_minutes'
			])}
			<div class="st-subsec-head">Refocus</div>
			{@render cfg([
				'steerer.refocus.enabled',
				'steerer.refocus.confidence_threshold',
				'steerer.refocus.debounce_turns'
			])}
			{@render cfg(['steerer.openrouter_api_key_env'])}
		</WorkerCard>
		{#if steererGated}<p class="st-gate" role="note">{gateHint}</p>{/if}
	{/if}

	<PageAlarms home="steering" />

	<div class="st-fleet-header">
		<h2 class="st-subhead">Steerer fleet<HelpHint slug="fleet" /></h2>
		<a href={resolve('/steering/new?mode=blank')} class="st-fleet-create"
			><Plus size={12} aria-hidden="true" />Create steerer</a
		>
	</div>

	{#if steerers.length > 0}
		<div class="st-fleet-bulk" data-augmented-ui="tl-clip br-clip border">
			<span class="st-fleet-bulk-label">Bulk model</span>
			<ModelPickerPopover
				currentValue=""
				allowInherit
				inheritLabel="(reset all to inherit default)"
				triggerLabel={bulkModelLoading ? 'Applying' : 'Set all steerers to'}
				disabled={bulkModelLoading}
				onSelect={(id) => void handleBulkModel(id)}
			/>
			{#if bulkModelResult !== null}<span class="st-fleet-bulk-result">{bulkModelResult}</span>{/if}
			{#if bulkModelError !== null}<span class="st-fleet-bulk-err" role="alert"
					>{bulkModelError}</span
				>{/if}
		</div>
	{/if}

	{#if steerersLoadError !== null}
		<div class="st-load-error" role="alert" data-augmented-ui="tl-clip br-clip border">
			<p class="st-err-msg">Could not load steerers: {steerersLoadError}</p>
			<span class="st-retry-backing"
				><button
					type="button"
					class="st-retry"
					data-augmented-ui="tr-clip bl-clip border"
					onclick={() => void loadFleet()}>Retry</button
				></span
			>
		</div>
	{:else if steerers.length === 0}
		<div class="st-fleet-empty" data-augmented-ui="tl-clip br-clip border">
			<p class="st-fleet-empty-msg">
				No steerers registered yet. Each one you create will inherit the default model above until
				you give it its own.
			</p>
			<Button variant="outline" href={resolve('/steering/new?mode=blank')}>
				<Plus class="size-3.5" aria-hidden="true" />
				<span>Create steerer</span>
			</Button>
		</div>
	{:else}
		<div class="st-fleet" data-augmented-ui="tl-clip br-clip border">
			{#each steerers as s (s.id)}
				<div class="st-fleet-row">
					<div class="st-fleet-info">
						<span class="st-fleet-name">{s.display_name}</span>
						{#if s.kind}<span class="st-fleet-kind">{s.kind}</span>{/if}
						<span class="st-fleet-meta"
							>{triggerLabel(s.trigger_point)} · {invocationLabel(s.invocation)}</span
						>
						<span class="st-fleet-scope">{s.project_path ?? 'Global'}</span>
						<Tooltip text={modelHierarchyHint(s)}>
							<span class="st-fleet-model-group">
								<span class="st-fleet-model">{effectiveModel(s)}</span>
								<span
									class="st-fleet-model-badge"
									class:st-fleet-model-badge--override={!isInherited(s)}
								>
									{isInherited(s) ? 'inherit' : 'override'}
								</span>
							</span>
						</Tooltip>
					</div>
					<div class="st-fleet-actions">
						{#if fleetToggleError[s.id]}
							<span class="st-fleet-err" role="alert" title={fleetToggleError[s.id] ?? ''}
								><TriangleAlert size={12} aria-hidden="true" /></span
							>
						{/if}
						{#if fleetModelError[s.id]}
							<span class="st-fleet-err" role="alert" title={fleetModelError[s.id] ?? ''}
								><TriangleAlert size={12} aria-hidden="true" /></span
							>
						{/if}
						<ModelPickerPopover
							currentValue={s.model}
							allowInherit
							inheritLabel="(inherit default model)"
							triggerLabel="Model"
							disabled={fleetModelLoading[s.id] ?? false}
							onSelect={(id) => void changeSteererModel(s, id)}
						/>
						<button
							type="button"
							class="st-ftoggle"
							class:st-ftoggle--on={s.enabled}
							aria-pressed={s.enabled}
							aria-label="{s.enabled ? 'Disable' : 'Enable'} {s.display_name}"
							disabled={fleetToggleLoading[s.id] ?? false}
							onclick={() => void toggleSteererEnabled(s)}
						>
							<span class="st-ftoggle__track" aria-hidden="true">
								<span class="st-ftoggle__thumb"></span>
							</span>
						</button>
						<a href={resolve(`/steering/${s.id}`)} class="st-fleet-edit">Edit</a>
					</div>
				</div>
			{/each}
		</div>
	{/if}
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
					disabled={steererGated}
					onSave={(k, v) => void handleSettingSave(k, v)}
				/>
			{/if}
		{/each}
	</div>
{/snippet}

<style>
	.st-subhead {
		font-family: var(--font-display);
		font-size: 12px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 6px 0 -4px;
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
		gap: 8px;
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
	.st-env-label {
		font-size: 12px;
		color: var(--text-2);
		flex: 1 1 auto;
		min-width: 0;
		overflow-wrap: break-word;
	}
	.st-env-ok {
		color: var(--status-success-fg);
	}
	.st-env-missing {
		color: var(--text-3);
	}
	.st-env-link {
		color: var(--text-3);
		text-decoration: underline;
		text-underline-offset: 2px;
		transition: color 0.15s;
	}
	.st-env-link:hover {
		color: var(--text-1);
	}
	.st-env-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}
	.st-gate {
		margin: 0;
		padding: 6px 12px;
		font-size: 11px;
		color: var(--status-warning-fg);
		background: var(--status-warning-bg);
		border: 1px solid var(--status-warning-border);
		border-radius: 4px;
		line-height: 1.5;
	}
	.st-cfg-list {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		overflow: hidden;
		background: var(--surface-card);
	}
	.st-model-block {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.st-model-label {
		font-family: inherit;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-strong);
	}
	.st-model-why {
		margin: -4px 0 0;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.6;
	}
	.st-model-err {
		margin: 0;
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
	}

	.st-fleet-header {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.st-fleet-header .st-subhead {
		flex: 1 1 auto;
	}
	.st-fleet-create {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		font-weight: 500;
		color: var(--interactive-active-fg);
		text-decoration: none;
		white-space: nowrap;
	}
	.st-fleet-create:hover {
		color: var(--text-strong);
	}
	.st-fleet {
		--edge: 8px;
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
		padding: calc(var(--edge) + 4px);
		background: var(--surface-card);
	}
	.st-fleet-row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border-subtle);
	}
	.st-fleet-row:last-child {
		border-bottom: none;
	}
	.st-fleet-info {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: 1 1 auto;
		min-width: 0;
		flex-wrap: wrap;
	}
	.st-fleet-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
		white-space: nowrap;
	}
	.st-fleet-kind {
		font-size: 10px;
		color: var(--text-3);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 3px;
		padding: 1px 5px;
		white-space: nowrap;
	}
	.st-fleet-meta {
		font-size: 11px;
		color: var(--text-3);
	}
	.st-fleet-scope {
		font-size: 10px;
		color: var(--text-3);
		font-family: var(--font-mono, monospace);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 3px;
		padding: 1px 5px;
		white-space: nowrap;
		overflow-wrap: anywhere;
	}
	.st-fleet-actions {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-shrink: 0;
	}
	.st-fleet-edit {
		font-size: 12px;
		color: var(--interactive-active-fg);
		text-decoration: none;
	}
	.st-fleet-edit:hover {
		color: var(--text-strong);
	}
	.st-fleet-err {
		display: inline-flex;
		align-items: center;
		color: var(--status-danger-fg);
		cursor: help;
	}
	.st-fleet-empty {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		text-align: center;
		padding: calc(var(--edge) + 12px) calc(var(--edge) + 4px);
		background: var(--surface-card);
	}
	.st-fleet-empty-msg {
		margin: 0;
		max-width: 46ch;
		font-size: 13px;
		color: var(--text-3);
		line-height: 1.6;
	}

	.st-fleet-model-group {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.st-fleet-model {
		font-size: 10px;
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 3px;
		padding: 1px 5px;
		white-space: nowrap;
		overflow-wrap: anywhere;
	}
	.st-fleet-model-badge {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		padding: 1px 5px;
		border-radius: 3px;
		border: 1px solid var(--status-muted-border);
		color: var(--status-muted-fg);
		background: var(--status-muted-bg);
		white-space: nowrap;
	}
	.st-fleet-model-badge--override {
		border-color: var(--status-steering-border);
		color: var(--status-steering-fg);
		background: var(--status-steering-bg);
	}

	.st-fleet-bulk {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
		padding: calc(var(--edge) + 2px) calc(var(--edge) + 6px);
		background: var(--surface-card);
	}
	.st-fleet-bulk-label {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}
	.st-fleet-bulk-result {
		font-size: 11px;
		color: var(--status-success-fg);
	}
	.st-fleet-bulk-err {
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	.st-ftoggle {
		background: none;
		border: none;
		cursor: pointer;
		padding: 2px;
		display: inline-flex;
		align-items: center;
		border-radius: 4px;
	}
	.st-ftoggle:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}
	.st-ftoggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.st-ftoggle__track {
		display: block;
		position: relative;
		width: 28px;
		height: 16px;
		border-radius: 8px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		transition:
			background 0.15s,
			border-color 0.15s;
	}
	.st-ftoggle--on .st-ftoggle__track {
		background: var(--status-success-bg);
		border-color: var(--status-success-fg);
		box-shadow: 0 0 0 1px var(--status-success-border);
	}
	.st-ftoggle__thumb {
		display: block;
		position: absolute;
		top: 2px;
		left: 2px;
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--text-3);
		transition:
			transform 0.15s,
			background 0.15s;
	}
	.st-ftoggle--on .st-ftoggle__thumb {
		transform: translateX(12px);
		background: var(--status-success-fg);
		box-shadow: 0 0 5px var(--status-success-border);
	}
</style>
