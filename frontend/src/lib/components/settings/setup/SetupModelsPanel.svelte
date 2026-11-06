<script lang="ts">
	import { resolve } from '$app/paths';
	import { Select, type SelectOption } from '$lib/components/ui/select';
	import { Button } from '$lib/components/ui/button';
	import type { FeatureDto, SettingMeta, SettingValue } from '$lib/api/settings';
	import type { LlmProvider, LlmProvidersResponse } from '$lib/api/setup';
	import {
		PER_JOB,
		PROVIDER_ORDER,
		TIERS,
		modelRefs,
		normalizeRef,
		providerOf,
		runningJobs,
		tierDefault,
		usedBy,
		type ModelJob
	} from '$lib/settings/setupModels';
	import SettingRow from '../SettingRow.svelte';
	import SetupRow from './SetupRow.svelte';

	interface Props {
		providers: LlmProvidersResponse | null;
		providersError: string | null;
		settings: SettingMeta[];
		features: FeatureDto[];
		onSave: (key: string, value: SettingValue) => Promise<void>;
	}

	const { providers, providersError, settings, features, onSave }: Props = $props();

	const LIMIT_KEYS = [
		'llm.daily_spend_ceiling_usd',
		'llm.ceiling_max_calls',
		'llm.rate_limit_per_minute'
	];

	let saving = $state<Record<string, boolean>>({});
	let errors = $state<Record<string, string | null>>({});

	async function save(key: string, value: SettingValue): Promise<void> {
		saving = { ...saving, [key]: true };
		errors = { ...errors, [key]: null };
		try {
			await onSave(key, value);
		} catch (reason) {
			errors = { ...errors, [key]: reason instanceof Error ? reason.message : String(reason) };
		} finally {
			saving = { ...saving, [key]: false };
		}
	}

	function text(key: string): string {
		const value = settings.find((s) => s.key === key)?.value;
		return typeof value === 'string' ? value.trim() : '';
	}

	function found(id: string): LlmProvider | undefined {
		return providers?.providers.find((p) => p.id === id);
	}

	const all = $derived(
		PROVIDER_ORDER.map((id) => found(id)).filter((p): p is LlmProvider => p !== undefined)
	);
	const available = $derived(all.filter((p) => p.available));
	const jobs = $derived(runningJobs(features, settings));
	const mode = $derived(text('llm.provider') || providers?.llm_provider || PER_JOB);

	const modeOptions = $derived<SelectOption[]>([
		...available.map((p) => ({ value: p.id, label: `${p.label} for every job` })),
		{ value: PER_JOB, label: 'A provider and a model for each job' },
		...(mode !== PER_JOB && !available.some((p) => p.id === mode)
			? [{ value: mode, label: `${mode} (not available here)`, disabled: true }]
			: [])
	]);

	function providerOptions(current: string): SelectOption[] {
		const options = available.map((p) => ({ value: p.id, label: p.label }));
		return options.some((o) => o.value === current)
			? options
			: [...options, { value: current, label: `${current} (not available here)`, disabled: true }];
	}

	function modelOptions(provider: string, current: string): SelectOption[] {
		const entry = found(provider);
		const labels = new Map(entry?.models.map((m) => [m.ref, m.label]) ?? []);
		const refs = modelRefs(entry);
		const options = refs.map((ref) => ({ value: ref, label: labels.get(ref) ?? ref }));
		return current === '' || refs.includes(current)
			? options
			: [{ value: current, label: current }, ...options];
	}

	function tierModel(provider: string, tier: (typeof TIERS)[number]): string {
		return tierDefault(found(provider), settings, tier);
	}

	function jobProvider(job: ModelJob): string {
		const stored = text(job.key);
		const wanted = providerOf(stored);
		return stored !== '' && available.some((p) => p.id === wanted)
			? wanted
			: (available[0]?.id ?? '');
	}

	function jobModel(job: ModelJob, provider: string): string {
		const stored = text(job.key);
		return stored !== '' && providerOf(stored) === provider
			? normalizeRef(provider, stored)
			: normalizeRef(provider, tierModel(provider, job.tier));
	}

	function onSelect(handler: (value: string) => void): (event: Event) => void {
		return (event) => handler((event.currentTarget as HTMLSelectElement).value);
	}

	function changeJobProvider(job: ModelJob, provider: string): void {
		void save(job.key, normalizeRef(provider, tierModel(provider, job.tier)));
	}

	const limitSettings = $derived(
		LIMIT_KEYS.map((key) => settings.find((s) => s.key === key)).filter(
			(s): s is SettingMeta => s !== undefined
		)
	);
</script>

{#if providers === null}
	<SetupRow
		label="Providers on this machine"
		tone="fail"
		detail={providersError ?? 'The provider list could not be loaded.'}
	/>
{:else}
	{#each all as provider (provider.id)}
		<SetupRow
			label={provider.label}
			tone={provider.available ? 'ok' : 'info'}
			status={provider.available ? 'Available' : 'Not available'}
			detail={provider.available ? provider.detail : `${provider.detail} ${provider.fix}`.trim()}
		/>
	{/each}

	{#if jobs.length === 0}
		<SetupRow
			label="Background jobs"
			detail="No job you switched on calls a language model, so there is nothing to choose here."
		/>
	{:else if available.length === 0}
		<SetupRow
			label="Background jobs"
			tone="warn"
			detail="No provider is available on this machine, so the jobs cannot call a model."
		/>
	{:else}
		<SetupRow
			label="How jobs choose a model"
			detail="One provider for every job, or a provider and a model for each job."
		>
			<Select
				label="Choice"
				value={mode}
				options={modeOptions}
				disabled={saving['llm.provider']}
				onchange={onSelect((value) => void save('llm.provider', value))}
			/>
		</SetupRow>
		{#if errors['llm.provider']}
			<SetupRow label="Could not save" tone="fail" detail={errors['llm.provider'] ?? ''} />
		{/if}

		{#if mode === PER_JOB}
			{#each jobs as job (job.key)}
				{@const provider = jobProvider(job)}
				{@const model = jobModel(job, provider)}
				<SetupRow label={job.name} detail="Uses the {job.tier} tier by default.">
					<Select
						label="Provider"
						value={provider}
						options={providerOptions(provider)}
						disabled={saving[job.key]}
						onchange={onSelect((value) => changeJobProvider(job, value))}
					/>
					<Select
						label="Model"
						value={model}
						options={modelOptions(provider, model)}
						disabled={saving[job.key]}
						onchange={onSelect((value) => void save(job.key, normalizeRef(provider, value)))}
					/>
				</SetupRow>
				{#if errors[job.key]}
					<SetupRow label="Could not save {job.name}" tone="fail" detail={errors[job.key] ?? ''} />
				{/if}
			{/each}
		{:else}
			{#each TIERS as tier (tier)}
				{@const key = `llm.provider_models.${mode}.${tier}`}
				{@const model = normalizeRef(mode, tierModel(mode, tier))}
				<SetupRow label="Model for the {tier} tier" detail="Used by: {usedBy(jobs, tier)}.">
					<Select
						label="Model"
						value={model}
						options={modelOptions(mode, model)}
						disabled={saving[key]}
						onchange={onSelect((value) => void save(key, normalizeRef(mode, value)))}
					/>
				</SetupRow>
				{#if errors[key]}
					<SetupRow label="Could not save the {tier} tier" tone="fail" detail={errors[key] ?? ''} />
				{/if}
			{/each}
		{/if}
	{/if}

	{#each limitSettings as limit (limit.key)}
		<SettingRow
			setting={limit}
			compact
			loading={saving[limit.key] ?? false}
			error={errors[limit.key] ?? null}
			onSave={(key, value) => void save(key, value)}
		/>
	{/each}

	<SetupRow
		label="OpenRouter key"
		detail="Claude and Codex calls use your own plan, so the spend limits count OpenRouter calls only. The key is set on the Integrations tab."
	>
		<Button variant="outline" size="sm" href={resolve('/settings/integrations')}>
			Open Integrations
		</Button>
	</SetupRow>
{/if}
