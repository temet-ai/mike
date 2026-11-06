<script lang="ts">
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';

	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import WorkerCard from '$lib/components/settings/WorkerCard.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import SyncStatusPanel from '$lib/components/settings/SyncStatusPanel.svelte';
	import EvalAlarmList from '$lib/components/settings/EvalAlarmList.svelte';
	import { acknowledgeEvalAlarm, getEvalAlarms } from '$lib/api/evalMetrics';
	import type { EvalAlarmsResponse } from '$lib/api/evalMetrics';
	import { alarmsFor } from '$lib/components/judges/judges';
	import { ApiError } from '$lib/api';
	import {
		getFeatures,
		getSettings,
		getSyncStatus,
		patchFeature,
		patchSetting
	} from '$lib/api/settings';
	import type { FeatureDto, SettingMeta, SettingValue, SyncStatusDto } from '$lib/api/settings';
	import { getOpenRouterUsage } from '$lib/api/usage';
	import type { OpenRouterUsageDto } from '$lib/api/usage';
	import type { WorkersPageData } from './+page';

	const { data }: { data: WorkersPageData } = $props();

	let features = $state<FeatureDto[]>([]);
	let loadError = $state<string | null>(null);
	let settings = $state<SettingMeta[]>([]);
	let settingsLoadError = $state<string | null>(null);
	let syncStatus = $state<SyncStatusDto | null>(null);
	let syncStatusError = $state<string | null>(null);
	let rowLoading = $state<Record<string, boolean>>({});
	let rowError = $state<Record<string, string | null>>({});
	let settingRowLoading = $state<Record<string, boolean>>({});
	let settingRowError = $state<Record<string, string | null>>({});
	let openrouterUsage = $state<OpenRouterUsageDto | null>(null);
	let openrouterUsageError = $state<string | null>(null);
	let alarms = $state<EvalAlarmsResponse | null>(null);
	let alarmsError = $state<string | null>(null);

	$effect(() => {
		alarms = data.alarms;
		alarmsError = data.alarmsError;
	});

	async function acknowledgeAlarm(id: number, note: string): Promise<void> {
		await acknowledgeEvalAlarm(id, note);
		alarms = await getEvalAlarms({ state: 'open' });
	}

	$effect(() => {
		features = data.features ?? [];
		loadError = data.loadError;
		settings = data.settings ?? [];
		settingsLoadError = data.settingsLoadError;
		syncStatus = data.syncStatus;
		syncStatusError = data.syncStatusError;
	});

	onMount(() => {
		if (data.loadError !== null) void loadFeatures();
		if (data.settingsLoadError !== null) void loadSettings();
		if (data.syncStatusError !== null) void loadSyncStatus();
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
			'/api/sync/status',
			() => {
				void loadSyncStatus();
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
	async function loadSyncStatus(): Promise<void> {
		syncStatusError = null;
		try {
			syncStatus = await getSyncStatus();
		} catch (err) {
			syncStatusError = err instanceof ApiError ? err.message : String(err);
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

	const byKey = $derived(new Map(settings.map((s) => [s.key, s])));
	const featureByKey = $derived(new Map(features.map((f) => [f.key, f])));
	function featureEnabled(key: string): boolean {
		return featureByKey.get(key)?.enabled ?? true;
	}

	const openrouterConfigured = $derived(openrouterUsage?.configured === true);
	const workerAlarms = $derived(alarmsFor(alarms?.alarms ?? [], data.bundles ?? [], 'workers'));
	function appliesFor(key: string): string | undefined {
		return features.find((f) => f.key === key)?.applies;
	}
</script>

<svelte:head><title>Workers — Settings — mike</title></svelte:head>

<SettingsPage
	title="Workers"
	current="/settings/workers"
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
		Background workers that run across all sessions.
		<span data-testid="judges-pointer"
			>How <a class="st-env-link" href={resolve('/sessions/labelling')}>turns</a> and
			<a class="st-env-link" href={`${resolve('/sessions/labelling')}?for=sessions`}>sessions</a> are
			labelled is set on the Labelling page under Sessions.</span
		>
		What they cost is on the
		<a class="st-env-link" href={resolve('/settings/integrations') + '#openrouter'}
			>Integrations page</a
		>.
	{/snippet}

	<div class="st-env" data-augmented-ui="tl-clip br-clip border">
		<div class="st-env-status" aria-live="polite">
			<span class="st-env-dot" class:st-env-dot--ok={openrouterConfigured} aria-hidden="true"
			></span>
			<span class="st-env-label"
				>OpenRouter API key, used by the lesson relation detector:
				{#if openrouterUsageError !== null}
					<span class="st-env-missing">could not check ({openrouterUsageError})</span>
				{:else if openrouterUsage === null}
					<span class="st-env-missing">checking</span>
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

	<h2 class="sr-only">Worker list</h2>

	<EvalAlarmList alarms={workerAlarms} error={alarmsError} onAcknowledge={acknowledgeAlarm} />

	<WorkerCard
		name="lesson_worker"
		label="Lesson worker"
		description="Merges lessons found in sessions into project memory and removes duplicates. Runs on this machine."
		applies={appliesFor('lesson_worker')}
		enabled={featureEnabled('lesson_worker')}
		onToggle={(v) => void handleFeatureToggle('lesson_worker', v)}
		error={rowError['lesson_worker'] ?? null}
		accent="info"
	>
		{@render cfg([
			'lesson_worker.dedup_threshold',
			'lesson_worker.max_lessons_per_project',
			'lesson_worker.session_consolidation_idle_secs',
			'lesson_worker.nightly_consolidation_hour',
			'lesson_worker.gc_interval_secs'
		])}
		{#if byKey.has('lesson_relations.enabled')}
			<div class="st-subsec-head">Relation detector</div>
			{@render cfg([
				'lesson_relations.enabled',
				'lesson_relations.proposer_model',
				'lesson_relations.adjudicator_model',
				'lesson_relations.candidate_distance_threshold'
			])}
		{/if}
	</WorkerCard>

	<WorkerCard
		name="harvester"
		label="Harvester"
		description="Finds lessons in finished turns: corrections, conventions and warnings."
		applies={appliesFor('harvester')}
		enabled={featureEnabled('harvester')}
		onToggle={(v) => void handleFeatureToggle('harvester', v)}
		error={rowError['harvester'] ?? null}
		accent="info"
	/>

	<WorkerCard
		name="retention_sweep"
		label="Retention sweep"
		description="Removes stale sessions and undelivered messages from the capture database. Off, nothing is removed and the database grows."
		applies={appliesFor('retention_sweep')}
		enabled={featureEnabled('retention_sweep')}
		onToggle={(v) => void handleFeatureToggle('retention_sweep', v)}
		error={rowError['retention_sweep'] ?? null}
		accent="info"
	/>

	<WorkerCard
		name="db_resync"
		label="Local DB resync"
		description="Copies rows captured locally while the remote database was unreachable back into it. Runs once at startup."
		applies={appliesFor('db_resync')}
		enabled={featureEnabled('db_resync')}
		onToggle={(v) => void handleFeatureToggle('db_resync', v)}
		error={rowError['db_resync'] ?? null}
		accent="info"
	>
		{#if syncStatusError !== null}
			<p class="st-sync-msg st-sync-msg--error" role="alert">
				Could not load sync status: {syncStatusError}
			</p>
		{:else if syncStatus !== null}
			<SyncStatusPanel status={syncStatus} />
		{:else}
			<p class="st-sync-msg">Loading sync status...</p>
		{/if}
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
	.st-sync-msg {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
	}
	.st-sync-msg--error {
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
</style>
