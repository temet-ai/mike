<script lang="ts">
	import {
		ArrowRight,
		CircleCheck,
		CircleDashed,
		CircleOff,
		Loader,
		TriangleAlert
	} from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import { StatusMark } from '$lib/components/ui/status-mark';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { formatRelative } from '$lib/time';
	import type { CapabilityRow } from '$lib/api/capabilities';
	import { COPY, formatClock, formatFull, type SectionState } from './labels';

	interface Props {
		mode: SectionState;
		capability: CapabilityRow | null;
		/** `measured_at` of the newest run for the project. */
		lastRun: string | null;
		now?: number;
	}

	const { mode, capability, lastRun, now = Date.now() }: Props = $props();

	const WORKERS = '/settings/workers#worker-knowledge_consolidation';
	const detail = $derived(capability?.detail ?? '');
	const needsFix = $derived((mode === 'degraded' || mode === 'inert') && !!capability?.fix);
	const off = $derived(mode === 'off-never' || mode === 'off-history');
</script>

<div class="wsl" data-slot="worker-status-line" data-mode={mode}>
	<div class="wsl__row">
		{#if off}
			<StatusMark icon={CircleOff} tone="quiet" label="Off" />
		{:else if mode === 'first-pass'}
			<StatusMark icon={Loader} tone="strong" label="First pass" spin />
		{:else if mode === 'degraded'}
			<StatusMark icon={TriangleAlert} tone="warning" label="Degraded" />
		{:else if mode === 'inert'}
			<StatusMark icon={CircleDashed} tone="warning" label="Inert" />
		{:else if mode === 'unknown'}
			<StatusMark icon={CircleDashed} tone="quiet" label="Unknown" />
		{:else}
			<StatusMark icon={CircleCheck} tone="strong" label="On" />
		{/if}

		{#if mode === 'off-never'}
			<span class="wsl__text">{COPY.offNever}</span>
		{:else if mode === 'unknown'}
			<span class="wsl__text">{COPY.unknown}</span>
		{:else}
			{#if mode === 'off-history'}
				<span class="wsl__text">{COPY.offHistory}</span>
			{/if}
			{#if mode !== 'first-pass'}
				{#if lastRun}
					<Tooltip text={formatFull(lastRun)}>
						<span class="wsl__run">
							Last run {formatClock(lastRun)}, {formatRelative(lastRun, now)}
						</span>
					</Tooltip>
				{:else}
					<span class="wsl__run">No run yet</span>
				{/if}
			{/if}
			{#if !off && detail}<span class="wsl__text">{detail}</span>{/if}
		{/if}

		<span class="wsl__spacer"></span>
		{#if off}
			<Button variant="default" size="sm" href={WORKERS}>Turn on in Workers settings</Button>
		{:else if mode !== 'unknown'}
			<Button
				variant={mode === 'degraded' || mode === 'inert' ? 'outline' : 'ghost'}
				size="sm"
				href={WORKERS}
			>
				Worker settings <ArrowRight data-icon="inline-end" aria-hidden="true" />
			</Button>
		{/if}
	</div>
	{#if needsFix}
		<div class="wsl__fix">
			<span class="machine wsl__fix-label">To fix</span>
			<span class="wsl__fix-text">{capability?.fix}</span>
		</div>
	{/if}
</div>

<style>
	.wsl {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-2) var(--space-4);
	}
	.wsl__row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
	.wsl__run {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.wsl__text {
		font-size: var(--text-xs);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.wsl__spacer {
		flex: 1 1 0;
	}
	.wsl__fix {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		align-items: baseline;
	}
	.wsl__fix-label {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.wsl__fix-text {
		font-size: var(--text-xs);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
</style>
