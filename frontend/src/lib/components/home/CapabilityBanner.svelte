<script lang="ts">
	import { onMount } from 'svelte';
	import { discloseHeight } from '$lib/components/ui/motion';
	import { TriangleAlert, X } from '@lucide/svelte';
	import { getCapabilities } from '$lib/api/capabilities';
	import type { CapabilityRow } from '$lib/api/capabilities';
	import { capabilityBannerDismissal } from '$lib/stores/capabilityBannerDismissal.svelte';
	import { fixSegments } from './fixLinks';

	const ELIGIBLE_IDS = new Set([
		'steering',
		'semantic_recall',
		'llm_credit',
		'remote_control_link',
		'llm_role_health',
		'worker_toggles',
		'db_cost'
	]);

	let rows = $state<CapabilityRow[]>([]);

	onMount(() => {
		void getCapabilities()
			.then((all) => {
				rows = all.filter(
					(r) => ELIGIBLE_IDS.has(r.id) && (r.state === 'inert' || r.state === 'degraded')
				);
			})
			.catch(() => {
				rows = [];
			});
	});

	const visibleRows = $derived(
		rows.filter((r) => !capabilityBannerDismissal.isDismissed(r.id, r.state))
	);
</script>

{#each visibleRows as row (row.id + ':' + row.state)}
	<div
		class="cap-banner"
		data-augmented-ui="tl-clip br-clip border"
		transition:discloseHeight
		role="status"
		aria-live="polite"
		data-slot="capability-banner"
		data-capability={row.id}
	>
		<div class="cap-banner__icon" aria-hidden="true">
			<TriangleAlert class="size-4" />
		</div>
		<div class="cap-banner__body">
			<strong class="cap-banner__title" data-slot="capability-banner-title">
				{row.label} is {row.state}
			</strong>
			<p class="cap-banner__detail">{row.detail}</p>
			{#if row.fix}
				<p class="cap-banner__fix">
					{#each fixSegments(row.fix) as segment, i (i)}
						{#if segment.kind === 'link'}
							<a
								class="cap-banner__link"
								href={segment.href}
								target={segment.external ? '_blank' : undefined}
								rel={segment.external ? 'noopener noreferrer' : undefined}>{segment.text}</a
							>
						{:else}
							{segment.text}
						{/if}
					{/each}
				</p>
			{/if}
		</div>
		<button
			type="button"
			class="cap-banner__dismiss"
			data-motion="hover press"
			aria-label="Dismiss {row.label} notice"
			onclick={() => capabilityBannerDismissal.dismiss(row.id, row.state)}
			data-slot="capability-banner-dismiss"
		>
			<X class="size-4" aria-hidden="true" />
		</button>
	</div>
{/each}

<style>
	.cap-banner {
		--aug-tl: var(--edge-sm);
		--aug-br: var(--edge-sm);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--status-warning-border);
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
		padding: calc(var(--edge-sm) + var(--space-1)) var(--space-4);
		background: var(--status-warning-bg);
		border-radius: var(--r-tol);
		font-size: var(--text-md);
		color: var(--status-warning-fg);
		margin-bottom: 8px;
	}
	.cap-banner__icon {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		margin-top: 1px;
	}
	.cap-banner__body {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.cap-banner__title {
		font-weight: 600;
		color: inherit;
		text-transform: capitalize;
	}
	.cap-banner__detail {
		margin: 0;
		font-size: 0.75rem;
		color: inherit;
		opacity: 0.9;
		line-height: 1.5;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.cap-banner__link {
		color: inherit;
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.cap-banner__fix {
		margin: 0;
		font-size: 0.75rem;
		font-weight: 600;
		color: inherit;
		line-height: 1.5;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.cap-banner__dismiss {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		width: var(--control-h);
		height: var(--control-h);
		padding: 0;
		border: none;
		background: transparent;
		color: inherit;
		opacity: 0.7;
		cursor: pointer;
		border-radius: var(--r-tol);
	}
	.cap-banner__dismiss:hover {
		opacity: 1;
		background: var(--surface-2);
	}
	.cap-banner__dismiss:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		opacity: 1;
	}
	@media (pointer: coarse) {
		.cap-banner__dismiss {
			width: calc(var(--control-h-lg) + var(--space-1));
			height: calc(var(--control-h-lg) + var(--space-1));
		}
	}
</style>
