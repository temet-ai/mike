<script lang="ts">
	import { resolve } from '$app/paths';
	import { Network } from '@lucide/svelte';
	import type { SessionFleetDto } from './types';

	interface Props {
		sessionId: string;
		project: string | null;
		dto: SessionFleetDto | null;
		unavailable: boolean;
	}

	const { sessionId, project, dto, unavailable }: Props = $props();

	const fleetHref = $derived.by(() => {
		const params = new URLSearchParams();
		if (project) params.set('project', project);
		params.set('node', sessionId);
		if (dto) params.set('window', String(dto.window_hours));
		return resolve(`/fleet?${params.toString()}` as `/fleet?${string}`);
	});
</script>

<a
	class="comms-fleet"
	class:comms-fleet--muted={!dto}
	href={fleetHref}
	data-augmented-ui="tl-clip br-clip border"
>
	<Network size={13} aria-hidden="true" />
	{#if dto}
		<span class="comms-fleet__stat">
			<span class="comms-fleet__num">{dto.peer_count}</span> peers
		</span>
		<span class="comms-fleet__sep" aria-hidden="true">·</span>
		<span class="comms-fleet__stat">
			<span class="comms-fleet__num">{dto.message_count}</span> messages
		</span>
		<span class="comms-fleet__sep" aria-hidden="true">·</span>
		<span class="comms-fleet__stat">
			<span class="comms-fleet__num">{dto.fleet_size}</span> in fleet
		</span>
		<span class="comms-fleet__window">last {dto.window_hours}h</span>
	{:else if unavailable}
		<span class="comms-fleet__stat">fleet mini-map not live yet</span>
	{:else}
		<span class="comms-fleet__stat">loading fleet…</span>
	{/if}
</a>

<style>
	.comms-fleet {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 4px 10px;
		background: var(--surface-card);
		color: var(--text-2);
		text-decoration: none;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		transition: color var(--dur-1) var(--ease-snap);
	}

	.comms-fleet:hover {
		--aug-border-bg: var(--border-default);
		color: var(--text-strong);
	}

	.comms-fleet:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.comms-fleet--muted {
		color: var(--text-3);
	}

	.comms-fleet__num {
		font-weight: 700;
		color: var(--text-1);
	}

	.comms-fleet__sep {
		color: var(--text-3);
	}

	.comms-fleet__window {
		margin-left: 4px;
		color: var(--text-3);
	}
</style>
