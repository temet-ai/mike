<script lang="ts">
	import { getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import { Waypoints } from '@lucide/svelte';
	import type { SessionSteeringRow } from '$lib/api/steering';
	import { AUTHORITY_LABELS, STEERING_MODE_LABELS } from '$lib/steering';
	import type { Authority, SteeringMode } from '$lib/steering';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { formatRelative } from '$lib/time';
	import InjectedSteerCallout from './InjectedSteerCallout.svelte';

	interface Props {
		intervention: SessionSteeringRow;
		sessionBasePath: string;
		now: number;
	}

	let { intervention, sessionBasePath, now }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	let localOpen = $state(false);
	const collapseKey = $derived(
		`timeline-steer:${intervention.session_id}:${intervention.intervention_id}`
	);
	const open = $derived(collapseStore ? collapseStore.isExpanded(collapseKey, false) : localOpen);

	function toggle(): void {
		if (collapseStore) collapseStore.toggle(collapseKey);
		else localOpen = !localOpen;
	}

	function authorityStyle(a: Authority | null): string {
		switch (a) {
			case 'command':
				return 'color: var(--status-warning-fg); background: var(--status-warning-bg); border-color: var(--status-warning-border);';
			case 'redirect':
				return 'color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);';
			default:
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	function modeStyle(m: SteeringMode | null): string {
		return m === 'drive'
			? 'color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);'
			: 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
	}

	const displayName = $derived(
		intervention.steerer_display_name ?? intervention.steerer_name ?? 'unknown steerer'
	);
</script>

<div
	class="sm-root"
	data-slot="steer-marker"
	data-augmented-ui="tl-clip br-clip border"
	style="--aug-border-all: 1px; --aug-border-bg: linear-gradient(135deg, var(--status-steering-fg) 0%, var(--status-steering-border) 35%, var(--border-default) 70%);"
>
	<button
		type="button"
		class="sm-toggle"
		onclick={toggle}
		aria-expanded={open}
		aria-label={`${open ? 'Collapse' : 'Expand'} steer fired by ${displayName}`}
	>
		<div class="sm-head">
			<span class="sm-icon-box">
				<Waypoints size={12} aria-hidden="true" />
			</span>
			<span class="sm-label">Steer fired</span>
			<span class="sm-meta">
				<span class="sm-meta__name entity-name">{displayName}</span>
				<span class="sm-meta__sep" aria-hidden="true">→</span>
				<span>delivered to #{intervention.sent_request_id ?? '—'}</span>
				{#if intervention.authority}
					<span class="sm-chip" style={authorityStyle(intervention.authority)}>
						{AUTHORITY_LABELS[intervention.authority as Authority] ?? intervention.authority}
					</span>
				{/if}
				{#if intervention.mode}
					<span class="sm-chip" style={modeStyle(intervention.mode)}>
						{STEERING_MODE_LABELS[intervention.mode as SteeringMode] ?? intervention.mode}
					</span>
				{/if}
				<span class="sm-time" title={intervention.sent_at ?? ''}
					>{formatRelative(intervention.sent_at, now)}</span
				>
			</span>
		</div>
	</button>

	{#if open}
		<div class="sm-body">
			{#if intervention.off_thread_topic}
				<p class="sm-topic">{intervention.off_thread_topic}</p>
			{/if}
			<p class="sm-links">
				{#if intervention.sent_request_id !== null}
					<a
						class="sm-link"
						href={resolve(
							`${sessionBasePath.slice(1)}/requests/${intervention.sent_request_id}` as `sessions/${string}/requests/${string}`
						)}
					>
						delivered to request #{intervention.sent_request_id}
					</a>
				{/if}
				{#if intervention.trigger_request_id !== null}
					<a
						class="sm-link"
						href={resolve(
							`${sessionBasePath.slice(1)}/requests/${intervention.trigger_request_id}` as `sessions/${string}/requests/${string}`
						)}
					>
						triggered by request #{intervention.trigger_request_id}
					</a>
				{/if}
			</p>
			{#if intervention.payload_text}
				<InjectedSteerCallout payloadText={intervention.payload_text} />
			{/if}
		</div>
	{/if}
</div>

<style>
	.sm-root {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: 12px;
		display: flex;
		flex-direction: column;
		background: var(--status-steering-bg);
	}

	.sm-toggle {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px 12px;
		border: none;
		background: transparent;
		text-align: left;
		font: inherit;
		cursor: pointer;
		color: inherit;
		width: 100%;
	}

	.sm-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.sm-head {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.sm-icon-box {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		flex-shrink: 0;
		border-radius: 2px;
		background: var(--status-steering-bg);
		border: 1px solid var(--status-steering-border);
		color: var(--status-steering-fg);
	}

	.sm-label {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--status-steering-fg);
	}

	.sm-meta {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
	}

	.sm-meta__name {
		color: var(--text-1);
	}

	.sm-meta__sep {
		color: var(--text-faint);
	}

	.sm-chip {
		display: inline-flex;
		align-items: center;
		padding: 1px 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		flex-shrink: 0;
	}

	.sm-time {
		color: var(--text-3);
	}

	.sm-body {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 0 12px 12px 12px;
		border-top: 1px solid var(--status-steering-border);
		padding-top: 10px;
	}

	.sm-topic {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sm-links {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
	}

	.sm-link {
		color: var(--text-2);
		text-decoration: underline;
		text-decoration-color: var(--border-default);
		text-underline-offset: 2px;
		width: fit-content;
	}

	.sm-link:hover {
		color: var(--text-1);
	}

	.sm-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}
</style>
