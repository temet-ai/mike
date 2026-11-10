<script lang="ts">
	import { onMount } from 'svelte';
	import { Play } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import SettingsSection from '$lib/components/settings/SettingsSection.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import { ApiError } from '$lib/api';
	import { getSettings, patchSetting } from '$lib/api/settings';
	import type { SettingMeta, SettingValue } from '$lib/api/settings';
	import { describeStopped, describeTuned, runRerankPass } from '$lib/api/rerank';
	import type { RerankPassResult } from '$lib/api/rerank';
	import type { RerankerPageData } from './+page';

	const { data }: { data: RerankerPageData } = $props();

	const TUNING_KEY = 'rerank.tuning.enabled';
	const NIGHT_GROUP = 'Overnight reranking';

	let settings = $state<SettingMeta[]>([]);
	let settingsLoadError = $state<string | null>(null);
	let settingRowLoading = $state<Record<string, boolean>>({});
	let settingRowError = $state<Record<string, string | null>>({});
	let passRunning = $state(false);
	let passResult = $state<RerankPassResult | null>(null);
	let passError = $state<string | null>(null);

	$effect(() => {
		settings = data.settings ?? [];
		settingsLoadError = data.settingsLoadError;
	});

	onMount(() => {
		if (data.settingsLoadError !== null) void loadSettings();
		return onApiCacheUpdate(
			'/api/settings',
			() => {
				void loadSettings();
			},
			{ prefix: true }
		);
	});

	async function loadSettings(): Promise<void> {
		settingsLoadError = null;
		try {
			const r = await getSettings();
			settings = r.settings;
		} catch (err) {
			settingsLoadError = err instanceof ApiError ? err.message : String(err);
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

	// Every reranker and search-ranking row the API serves, grouped by the group the row carries.
	const sections = $derived.by(() => {
		const groups: [string, SettingMeta[]][] = [];
		for (const s of settings) {
			if (!s.key.startsWith('rerank.') && !s.key.startsWith('search.ranking.')) continue;
			const group = groups.find(([name]) => name === s.group);
			if (group) group[1].push(s);
			else groups.push([s.group, [s]]);
		}
		return groups;
	});

	const tuning = $derived(settings.find((s) => s.key === TUNING_KEY));
	const tuningOn = $derived(tuning?.value === true);
	const disabledReason = $derived(
		tuningOn ? null : `Turn on “${tuning?.label ?? TUNING_KEY}” to run a pass.`
	);

	const passSummary = $derived(
		passResult === null
			? ''
			: `Graded ${passResult.graded} searches; tuning: ${describeTuned(passResult.tuned)}; ` +
					`${passResult.thread_candidates} thread candidates; ${passResult.lesson_pairs} lesson pairs` +
					(passResult.stopped === null
						? '.'
						: `; stopped because ${describeStopped(passResult.stopped)}.`)
	);

	async function runPass(): Promise<void> {
		passRunning = true;
		passResult = null;
		passError = null;
		try {
			passResult = await runRerankPass();
		} catch (err) {
			passError = err instanceof ApiError ? err.message : String(err);
		} finally {
			passRunning = false;
		}
	}
</script>

<svelte:head><title>Reranker — Settings — mike</title></svelte:head>

<SettingsPage
	title="Reranker"
	current="/settings/reranker"
	loadErrors={[settingsLoadError === null ? null : `Settings: ${settingsLoadError}`]}
	onRetry={() => {
		void loadSettings();
	}}
>
	{#each sections as [group, rows] (group)}
		<SettingsSection title={group}>
			{#if group === NIGHT_GROUP}
				<div class="rr-run">
					<Button
						variant="outline"
						disabled={!tuningOn || passRunning}
						aria-busy={passRunning}
						onclick={() => void runPass()}
					>
						<Play aria-hidden="true" />
						{passRunning ? 'Running overnight pass' : 'Run overnight pass now'}
					</Button>
					<p class="rr-run-status" role="status" aria-live="polite">
						{#if passError !== null}
							<span class="rr-run-error">{passError}</span>
						{:else if passResult !== null}
							{passSummary}
						{:else if disabledReason !== null}
							{disabledReason}
						{:else if passRunning}
							This can take minutes.
						{/if}
					</p>
				</div>
			{/if}
			<div class="rr-rows">
				{#each rows as s (s.key)}
					<SettingRow
						setting={s}
						loading={settingRowLoading[s.key] ?? false}
						error={settingRowError[s.key] ?? null}
						onSave={(k, v) => void handleSettingSave(k, v)}
					/>
				{/each}
			</div>
		</SettingsSection>
	{/each}
</SettingsPage>

<style>
	.rr-run {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) calc(var(--edge-md) + var(--space-2));
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.rr-run-status {
		flex: 1 1 16rem;
		min-height: 1.55em;
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
		line-height: var(--leading-body);
		overflow-wrap: anywhere;
	}
	.rr-run-error {
		color: var(--status-danger-fg);
	}
	.rr-rows {
		display: flex;
		flex-direction: column;
	}
</style>
