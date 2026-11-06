<script lang="ts">
	import { CircleCheck, TriangleAlert, CircleDashed, CircleOff, ArrowRight } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import SettingsSection from './SettingsSection.svelte';
	import type { CapabilityRow, CapabilityState } from '$lib/api/capabilities';

	interface Props {
		capabilities: CapabilityRow[];
	}

	const { capabilities }: Props = $props();

	const STATE_LABEL: Record<CapabilityState, string> = {
		active: 'Active',
		degraded: 'Degraded',
		inert: 'Inert',
		off: 'Off'
	};

	const CONFIG_PAGE: Record<
		string,
		{ href: '/settings/steering' | '/settings/embedding' | '/settings/flagging'; label: string }
	> = {
		steering: { href: '/settings/steering', label: 'Steering settings' },
		semantic_recall: { href: '/settings/embedding', label: 'Embedding settings' },
		weasel_intercept: { href: '/settings/flagging', label: 'Flagging settings' }
	};
</script>

<SettingsSection title="Capability status" id="capability-status" testId="capability-panel">
	{#snippet description()}
		What each subsystem is actually doing right now, not just whether its toggle is on.
		<strong>Inert</strong> means a feature reports enabled but isn't doing anything — usually a missing
		credential or empty config.
	{/snippet}

	{#each capabilities as row (row.id)}
		{@const configPage = CONFIG_PAGE[row.id]}
		<div
			class="cap-row"
			data-testid="capability-row"
			data-capability={row.id}
			data-state={row.state}
		>
			<div class="cap-row-inner">
				<span class="cap-badge cap-badge--{row.state}">
					{#if row.state === 'active'}
						<CircleCheck class="size-3" aria-hidden="true" />
					{:else if row.state === 'degraded'}
						<TriangleAlert class="size-3" aria-hidden="true" />
					{:else if row.state === 'inert'}
						<CircleDashed class="size-3" aria-hidden="true" />
					{:else}
						<CircleOff class="size-3" aria-hidden="true" />
					{/if}
					<span>{STATE_LABEL[row.state]}</span>
				</span>

				<div class="cap-body">
					<span class="cap-label">{row.label}</span>
					<p class="cap-detail">{row.detail}</p>
					{#if row.state !== 'active' && row.fix}
						<p class="cap-fix"><span class="cap-fix-lead">Fix:</span> {row.fix}</p>
					{/if}
				</div>

				{#if configPage}
					<a class="cap-config" href={resolve(configPage.href)}>
						{configPage.label}
						<ArrowRight class="size-3" aria-hidden="true" />
					</a>
				{/if}
			</div>
		</div>
	{/each}
</SettingsSection>

<style>
	.cap-row {
		padding: 12px 20px;
		border-bottom: 1px solid var(--border-subtle);
		container-type: inline-size;
	}
	.cap-row:last-child {
		border-bottom: none;
	}
	.cap-row-inner {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}
	@container (max-width: 460px) {
		.cap-row-inner {
			flex-direction: column;
			align-items: flex-start;
			gap: 8px;
		}
	}

	.cap-badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
		margin-top: 1px;
		padding: 2px 8px;
		border-radius: 9999px;
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.cap-badge--active {
		background: var(--status-success-bg);
		color: var(--status-success-fg);
		box-shadow: inset 0 0 0 1px var(--status-success-border);
	}
	.cap-badge--degraded {
		background: var(--status-warning-bg);
		color: var(--status-warning-fg);
		box-shadow: inset 0 0 0 1px var(--status-warning-border);
	}
	.cap-badge--inert {
		background: transparent;
		color: var(--status-warning-fg);
		box-shadow: inset 0 0 0 1px var(--status-warning-border);
		border: 1px dashed var(--status-warning-border);
	}
	.cap-badge--off {
		background: var(--status-muted-bg);
		color: var(--status-muted-fg);
		box-shadow: inset 0 0 0 1px var(--status-muted-border);
	}

	.cap-body {
		display: flex;
		flex-direction: column;
		gap: 3px;
		flex: 1 1 0;
		min-width: 0;
	}
	.cap-label {
		font-family: inherit;
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
	}
	.cap-detail {
		margin: 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.cap-fix {
		margin: 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--status-warning-fg);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.cap-fix-lead {
		font-weight: 600;
	}

	.cap-config {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		flex-shrink: 0;
		margin-top: 1px;
		font-family: inherit;
		font-size: 11px;
		font-weight: 600;
		white-space: nowrap;
		color: var(--text-3);
		text-decoration: none;
	}
	.cap-config:hover {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-decoration-color: var(--interactive-active-border);
		text-underline-offset: 3px;
	}
	.cap-config:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}
</style>
