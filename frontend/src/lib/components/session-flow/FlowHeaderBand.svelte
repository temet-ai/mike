<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import Badge from '$lib/components/shared/Badge.svelte';

	let {
		embedded = false,
		loading = false,
		sessionId,
		peerId,
		startedAt,
		agentCount,
		title,
		timelineHref,
		stale = false,
		staleReasons = []
	}: {
		embedded?: boolean;
		loading?: boolean;
		sessionId: string;
		peerId: string | null;
		startedAt: string | null;
		agentCount: number;
		title: string | null;
		timelineHref: string;
		stale?: boolean;
		staleReasons?: readonly string[];
	} = $props();

	function formatDateDDMonYYYY(iso: string | null): string {
		if (!iso) return 'Unknown date';
		const d = new Date(iso);
		if (Number.isNaN(d.getTime())) return 'Unknown date';
		const day = String(d.getDate()).padStart(2, '0');
		const month = d.toLocaleDateString('en-US', { month: 'short' });
		return `${day} ${month} ${d.getFullYear()}`;
	}

	const identityLine = $derived(
		`${sessionId}${peerId ? ` + peer ${peerId}` : ''} · ${formatDateDDMonYYYY(startedAt)} · ${agentCount} DELEGATED AGENTS`
	);
</script>

<SectionCard
	title="Session flow"
	bordered={false}
	expandable={false}
	description={embedded ? undefined : (title ?? undefined)}
>
	{#snippet headerActions()}
		<span class="flow-header-band__meta"
			>{embedded ? `${agentCount} DELEGATED AGENTS` : identityLine}</span
		>
		<span class="flow-header-band__loading" role="status" aria-live="polite">
			<span class:invisible={!loading}>Loading flow data…</span>
		</span>
		<span class:invisible={!stale}
			><Badge variant="chip-warning" title={staleReasons.join(' · ')}>STALE</Badge></span
		>
		{#if !embedded}<Button variant="outline" size="sm" href={timelineHref}>Timeline ↗</Button>{/if}
	{/snippet}
	<p>This band has no body.</p>
</SectionCard>

<style>
	.flow-header-band__meta {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.flow-header-band__loading {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-3);
		white-space: nowrap;
	}
</style>
