<script lang="ts">
	import * as RadioGroup from '$lib/components/ui/radio-group';
	import { Button } from '$lib/components/ui/button';
	import type { FeatureDto } from '$lib/api/settings';
	import {
		currentPreset,
		switchesToChange,
		switchesTurnedOff,
		type FeaturePreset
	} from '$lib/api/setup';
	import SetupRow from './SetupRow.svelte';

	interface Props {
		presets: FeaturePreset[];
		features: FeatureDto[];
		onApply: (preset: FeaturePreset) => Promise<number>;
	}

	const { presets, features, onApply }: Props = $props();

	const COPY: Record<string, { label: string; description: string }> = {
		'watch-only': {
			label: 'Watch only',
			description:
				'mike records and shows the traffic and changes none of it. Every switch that rewrites a request or sends captured text to another service is off.'
		},
		recommended: {
			label: 'Recommended',
			description:
				'Watch only, plus recall, lessons and reminders. They add notes to what Claude Code sends to Anthropic, and nothing leaves this machine for them.'
		},
		everything: {
			label: 'Everything',
			description:
				'Every switch on, including the ones that send captured text to a language model and the ones that rewrite requests and responses. The ones that call a model do nothing until a provider is set up.'
		}
	};

	const current = $derived(currentPreset(presets, features));
	let picked = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);
	let result = $state<string | null>(null);

	$effect(() => {
		picked = current ?? '';
	});

	const chosen = $derived(presets.find((p) => p.id === picked) ?? null);
	const turnsOff = $derived(chosen ? switchesTurnedOff(chosen, features) : []);
	const changeCount = $derived(chosen ? switchesToChange(chosen, features).length : 0);

	async function apply(): Promise<void> {
		if (!chosen || busy) return;
		busy = true;
		error = null;
		result = null;
		try {
			const changed = await onApply(chosen);
			result = changed === 1 ? 'Changed 1 switch.' : `Changed ${changed} switches.`;
		} catch (reason) {
			error = reason instanceof Error ? reason.message : String(reason);
		} finally {
			busy = false;
		}
	}
</script>

<SetupRow
	label="Which features run"
	tone={current === null ? 'warn' : 'ok'}
	status={current === null ? 'Custom' : (COPY[current]?.label ?? current)}
	detail={current === null
		? 'The switches match none of the three choices. Applying one changes only the switches that differ.'
		: 'The switches match this choice. Single switches are on the General tab.'}
	wide
>
	<RadioGroup.Root name="setup-feature-preset" bind:value={picked} aria-label="Feature choice">
		{#each presets as preset (preset.id)}
			<RadioGroup.Item
				value={preset.id}
				label={COPY[preset.id]?.label ?? preset.id}
				description={COPY[preset.id]?.description}
			/>
		{/each}
	</RadioGroup.Root>
</SetupRow>
<SetupRow
	label="Apply the choice"
	detail={turnsOff.length > 0
		? `It turns off these switches that are on now: ${turnsOff.join(', ')}.`
		: undefined}
>
	<div class="setup-apply">
		<Button disabled={busy || chosen === null || picked === current} onclick={() => void apply()}>
			{busy ? 'Applying' : changeCount === 0 ? 'Apply' : `Apply, ${changeCount} to change`}
		</Button>
		<p class="setup-result" role="status" aria-live="polite">{result ?? ''}</p>
	</div>
	{#if error !== null}
		<p class="setup-error" role="alert">{error}</p>
	{/if}
</SetupRow>

<style>
	.setup-apply {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}
	.setup-result,
	.setup-error {
		margin: 0;
		min-height: 1.55em;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.setup-error {
		color: var(--status-danger-fg);
	}
</style>
