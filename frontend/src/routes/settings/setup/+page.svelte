<script lang="ts">
	import { RefreshCw } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import SettingsSection from '$lib/components/settings/SettingsSection.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import LaunchBackendValidationCard from '$lib/components/settings/LaunchBackendValidationCard.svelte';
	import SetupRow from '$lib/components/settings/setup/SetupRow.svelte';
	import SetupFeaturesPanel from '$lib/components/settings/setup/SetupFeaturesPanel.svelte';
	import SetupModelsPanel from '$lib/components/settings/setup/SetupModelsPanel.svelte';
	import { Button } from '$lib/components/ui/button';
	import { getFeatures, getSettings, patchSetting } from '$lib/api/settings';
	import type { FeatureDto, SettingMeta, SettingValue } from '$lib/api/settings';
	import {
		applyFeaturePreset,
		getLlmProviders,
		getSetupState,
		type FeaturePreset,
		type LlmProvidersResponse,
		type SetupCheck,
		type SetupState
	} from '$lib/api/setup';
	import {
		SETUP_STEP_COMMAND,
		checksForStep,
		generalChecks,
		type SetupStepId
	} from '$lib/settings/setupSteps';
	import type { SetupPageData } from './+page';

	const { data }: { data: SetupPageData } = $props();

	let setup = $state<SetupState | null>(null);
	let setupError = $state<string | null>(null);
	let features = $state<FeatureDto[]>([]);
	let settings = $state<SettingMeta[]>([]);
	let settingsError = $state<string | null>(null);
	let providers = $state<LlmProvidersResponse | null>(null);
	let providersError = $state<string | null>(null);
	let checking = $state(false);
	let rowLoading = $state<Record<string, boolean>>({});
	let rowError = $state<Record<string, string | null>>({});

	$effect(() => {
		setup = data.state;
		setupError = data.stateError;
		features = data.features ?? [];
		settings = data.settings ?? [];
		settingsError = data.settingsError;
		providers = data.providers;
		providersError = data.providersError;
	});

	function failure(reason: unknown): string {
		return reason instanceof Error ? reason.message : String(reason);
	}

	async function refresh(): Promise<void> {
		checking = true;
		const [state, feats, sets, provs] = await Promise.allSettled([
			getSetupState(),
			getFeatures(),
			getSettings(),
			getLlmProviders()
		]);
		if (state.status === 'fulfilled') {
			setup = state.value;
			setupError = null;
		} else setupError = failure(state.reason);
		if (feats.status === 'fulfilled') features = feats.value;
		if (sets.status === 'fulfilled') {
			settings = sets.value.settings;
			settingsError = null;
		} else settingsError = failure(sets.reason);
		if (provs.status === 'fulfilled') {
			providers = provs.value;
			providersError = null;
		} else providersError = failure(provs.reason);
		checking = false;
	}

	async function saveSetting(key: string, value: SettingValue): Promise<void> {
		const saved = await patchSetting(key, value);
		settings = settings.map((s) => (s.key === key ? { ...s, ...saved } : s));
	}

	async function saveRow(key: string, value: SettingValue): Promise<void> {
		rowLoading = { ...rowLoading, [key]: true };
		rowError = { ...rowError, [key]: null };
		try {
			await saveSetting(key, value);
		} catch (reason) {
			rowError = { ...rowError, [key]: failure(reason) };
		} finally {
			rowLoading = { ...rowLoading, [key]: false };
		}
	}

	async function applyPreset(preset: FeaturePreset): Promise<number> {
		const lessons = settings.find((s) => s.key === preset.lessons_setting)?.value;
		try {
			return await applyFeaturePreset(
				preset,
				features,
				typeof lessons === 'boolean' ? lessons : null
			);
		} finally {
			await refresh();
		}
	}

	function setting(key: string): SettingMeta | undefined {
		return settings.find((s) => s.key === key);
	}

	function stepCommands(step: SetupStepId, checks: SetupCheck[], always: boolean): string[] {
		const fixes = checks
			.filter((check) => check.status !== 'ok' && check.fix)
			.map((check) => check.fix as string);
		if (always) fixes.unshift(SETUP_STEP_COMMAND[step]);
		return [...new Set(fixes)];
	}

	const checks = $derived(setup?.doctor.checks ?? []);
	const claude = $derived(setup?.claude_code ?? null);
	const summary = $derived(
		setup === null
			? null
			: `${setup.doctor.version}: ${setup.doctor.ok} ok, ${setup.doctor.warn} warnings, ${setup.doctor.fail} failures.`
	);
</script>

<svelte:head><title>Setup — mike</title></svelte:head>

{#snippet checkRows(step: SetupStepId)}
	{#each checksForStep(checks, step) as check (check.id)}
		<SetupRow label={check.name} detail={check.detail} tone={check.status} />
	{/each}
{/snippet}

{#snippet terminalRow(step: SetupStepId, always: boolean)}
	{@const commands = stepCommands(step, checksForStep(checks, step), always)}
	{#if commands.length > 0}
		<SetupRow
			label={always ? 'Run this step in a terminal' : 'Fix it in a terminal'}
			detail="These need your terminal, so the browser does not run them."
			{commands}
		/>
	{/if}
{/snippet}

<SettingsPage
	title="Setup"
	current="/settings/setup"
	loadErrors={[
		setupError === null ? null : `Checks: ${setupError}`,
		settingsError === null ? null : `Settings: ${settingsError}`
	]}
	onRetry={() => void refresh()}
>
	{#snippet lede()}
		Each step of <code>mike setup</code>, with what mike found on this machine. Change what the
		browser can here and copy the command for the rest.
	{/snippet}

	<SettingsSection title="Checks" id="checks">
		{#snippet action()}
			<Button variant="outline" size="sm" disabled={checking} onclick={() => void refresh()}>
				<RefreshCw size={13} aria-hidden="true" />
				{checking ? 'Checking' : 'Check again'}
			</Button>
		{/snippet}
		<SetupRow
			label="mike doctor"
			tone={setup === null ? 'info' : setup.doctor.passed ? 'ok' : 'fail'}
			status={setup === null ? 'No result' : setup.doctor.passed ? 'Passed' : 'Failed'}
			detail={summary ?? 'The checks could not be loaded.'}
			commands={['mike doctor']}
		/>
		{#each generalChecks(checks) as check (check.id)}
			<SetupRow label={check.name} detail={check.detail} tone={check.status} />
		{/each}
	</SettingsSection>

	<SettingsSection title="Database" id="setup-database">
		{@render checkRows('database')}
		{@render terminalRow('database', true)}
	</SettingsSection>

	<SettingsSection title="Background service" id="setup-service">
		{@render checkRows('service')}
		{@render terminalRow('service', true)}
	</SettingsSection>

	<SettingsSection title="Claude Code" id="setup-claude">
		{@render checkRows('claude')}
		{#if claude === null}
			<SetupRow
				label="Claude Code wiring"
				tone="warn"
				detail="The route, folder trust and mods could not be read from the config file."
			/>
		{:else}
			<SetupRow
				label="Route"
				tone={claude.route === null ? 'warn' : 'ok'}
				status={claude.route === null ? 'Not chosen' : claude.route}
				detail="How Claude Code reaches mike. The sidecar route also captures Remote Control traffic. The plain route needs no certificate."
				commands={['mike claude-code route sidecar', 'mike claude-code route plain']}
			/>
			<SetupRow
				label="Folder trust"
				tone="info"
				status={claude.auto_trust === true ? 'On' : 'Off'}
				detail="When on, mike claude marks the folder as trusted in ~/.claude.json before it starts. When off, Claude Code asks about each new folder itself."
				commands={['mike claude-code trust on', 'mike claude-code trust off']}
			/>
			<SetupRow
				label="Mods"
				tone={claude.mods_in_place ? 'ok' : 'info'}
				status={claude.mods_in_place ? 'Installed' : 'Not installed'}
				detail="The panes and status line mike adds inside Claude Code."
				commands={['mike claude-code mods']}
			/>
		{/if}
		{@render terminalRow('claude', true)}
	</SettingsSection>

	<SettingsSection title="Features" id="setup-features">
		{#if setup === null}
			<SetupRow
				label="Feature choices"
				tone="fail"
				detail={setupError ?? 'The feature choices could not be loaded.'}
			/>
		{:else}
			<SetupFeaturesPanel presets={setup.feature_presets} {features} onApply={applyPreset} />
		{/if}
	</SettingsSection>

	<SettingsSection title="Background models" id="setup-models">
		{@render checkRows('models')}
		<SetupModelsPanel {providers} {providersError} {settings} {features} onSave={saveSetting} />
		{@render terminalRow('models', false)}
	</SettingsSection>

	<SettingsSection title="Search by meaning" id="setup-embeddings">
		{@render checkRows('embeddings')}
		{#each ['embedding.enabled', 'embedding.url', 'embedding.model'] as key (key)}
			{@const found = setting(key)}
			{#if found}
				<SettingRow
					setting={found}
					compact
					loading={rowLoading[key] ?? false}
					error={rowError[key] ?? null}
					onSave={(k, v) => void saveRow(k, v)}
				/>
			{/if}
		{/each}
		<SetupRow
			label="Embedding settings"
			detail="Chunk sizes, the embedding graph and the full list are on the Embedding tab."
		>
			<Button variant="outline" size="sm" href={resolve('/settings/embedding')}>
				Open Embedding
			</Button>
		</SetupRow>
		{@render terminalRow('embeddings', true)}
	</SettingsSection>

	<SettingsSection title="Terminal" id="setup-terminal">
		{@render checkRows('terminal')}
		{@const backend = setting('launch_backend')}
		{#if backend}
			<SettingRow
				setting={backend}
				compact
				loading={rowLoading[backend.key] ?? false}
				error={rowError[backend.key] ?? null}
				onSave={(k, v) => void saveRow(k, v)}
			/>
			{#if typeof backend.value === 'string'}
				<LaunchBackendValidationCard backend={backend.value} />
			{/if}
		{/if}
		{@render terminalRow('terminal', false)}
	</SettingsSection>
</SettingsPage>
