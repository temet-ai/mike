<script lang="ts">
	import Badge from '$lib/components/shared/Badge.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
	import type { CompressionEffective } from '$lib/api/settings';

	interface Props {
		effective: CompressionEffective;
	}

	const { effective }: Props = $props();

	const state = $derived(effective.enabled === false ? 'off' : effective.state);

	const badge = $derived.by((): { label: string; variant: BadgeVariant } | null => {
		switch (state) {
			case 'pure_shadow':
				return { label: 'Shadow only', variant: 'chip-muted' };
			case 'sampled_forwarding':
				return {
					label: `Sampled forwarding: ${effective.forward_share}% of new sessions`,
					variant: 'chip-warning'
				};
			case 'forced_forwarding':
				return { label: 'Forwarding every session', variant: 'chip-warning' };
			case 'off':
				return { label: 'Off', variant: 'chip-muted' };
			default:
				return null;
		}
	});
</script>

<span class="crf" data-slot="compression-forwarding-state">
	{#if badge}
		{#if effective.session_arm}
			<Tooltip text={effective.session_arm}>
				<Badge variant={badge.variant}>{badge.label}</Badge>
			</Tooltip>
		{:else}
			<Badge variant={badge.variant}>{badge.label}</Badge>
		{/if}
	{/if}
	<span class="crf-summary">{effective.summary}</span>
</span>

<style>
	.crf {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
	}
</style>
