<script lang="ts">
	import { ShipWheel } from '@lucide/svelte';
	import { untrack } from 'svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import SessionSteeringSection from './SessionSteeringSection.svelte';
	import AgentsSteerersBand from './AgentsSteerersBand.svelte';
	import type { SteererEntry } from './AgentsSteerersBand.svelte';
	import type { AgentEntry } from '$lib/agentDispatches';
	import { getSessionEnrolments, getSessionSteering } from '$lib/api/steering';
	import type { SessionEnrolmentRow, SessionSteeringRow } from '$lib/api/steering';
	import { proxyEvents } from '$lib/ws.svelte';

	interface Props {
		sessionId: string;
		now: number;
		steerersEnrolled: number;
		steerersTotal: number;
		gradersActive: number;
		rulesActive: number;
		analyzersActive: number;
		agents: AgentEntry[];
		onJumpTo: (requestId: number) => void;
		grouped?: boolean;
	}

	let {
		sessionId,
		now,
		steerersEnrolled,
		steerersTotal,
		gradersActive,
		rulesActive,
		analyzersActive,
		agents,
		onJumpTo,
		grouped = false
	}: Props = $props();

	const scpExpandKey = $derived(`session:${sessionId}:controls-expanded`);

	let enrolments = $state<SessionEnrolmentRow[]>([]);
	let steeringRows = $state<SessionSteeringRow[]>([]);
	let steeringReloadKey = $state(0);

	let prevSteerersSid = '';
	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			if (sid === prevSteerersSid) return;
			prevSteerersSid = sid;
			enrolments = [];
			steeringRows = [];
		});
	});

	$effect(() => {
		const sid = sessionId;
		void steeringReloadKey;
		untrack(() => {
			if (!sid) return;
			getSessionEnrolments(sid)
				.then((rows) => {
					if (sessionId === sid) enrolments = rows;
				})
				.catch(() => {
					if (sessionId === sid) enrolments = [];
				});
			getSessionSteering(sid)
				.then((rows) => {
					if (sessionId === sid) steeringRows = rows;
				})
				.catch(() => {
					if (sessionId === sid) steeringRows = [];
				});
		});
	});

	let steerersRefetchTimer: ReturnType<typeof setTimeout> | null = null;
	let steerersProcessedEvents = 0;
	$effect(() => {
		const total = proxyEvents.totalEvents;
		untrack(() => {
			if (total === steerersProcessedEvents) return;
			const start = Math.max(steerersProcessedEvents, total - 200);
			for (let i = start; i < total; i++) {
				const ev = proxyEvents.at(i);
				if (!ev) continue;
				if (
					ev.type === 'RowChanged' &&
					ev.table === 'interventions' &&
					(ev.session_id === null || ev.session_id === sessionId)
				) {
					if (steerersRefetchTimer) clearTimeout(steerersRefetchTimer);
					steerersRefetchTimer = setTimeout(() => {
						steerersRefetchTimer = null;
						steeringReloadKey += 1;
					}, 800);
					break;
				}
			}
			steerersProcessedEvents = total;
		});
	});

	const steererEntries = $derived.by<SteererEntry[]>(() => {
		const byName = new Map<string, SteererEntry>();
		for (const e of enrolments) {
			if (e.disenrolled_at !== null) continue;
			const key = e.steerer_name ?? `enrolment-${e.enrolment_id}`;
			byName.set(key, {
				steererDisplayName: e.steerer_display_name,
				steererName: e.steerer_name,
				description: null,
				authority: e.authority,
				mode: e.mode,
				enrolled: true,
				events: []
			});
		}
		for (const row of steeringRows) {
			if (!row.verdict) continue;
			const key = row.steerer_name ?? `intervention-${row.intervention_id}`;
			let entry = byName.get(key);
			if (!entry) {
				entry = {
					steererDisplayName: row.steerer_display_name,
					steererName: row.steerer_name,
					description: null,
					authority: row.authority,
					mode: row.mode,
					enrolled: false,
					events: []
				};
				byName.set(key, entry);
			}
			entry.events.push({
				createdAt: row.created_at,
				verdict: row.verdict,
				confidence: row.confidence,
				gateReason: row.gate_reason,
				payloadText: row.payload_text,
				triggerRequestId: row.trigger_request_id,
				sentRequestId: row.sent_request_id
			});
		}
		return Array.from(byName.values());
	});
</script>

<SectionCard
	title="Session controls"
	titleIcon={ShipWheel}
	collapsible
	bordered={!grouped}
	sectionKey={scpExpandKey}
	defaultCollapsed
>
	{#snippet headerActions()}
		<span class="scp-meta">
			{steerersEnrolled}/{steerersTotal} steerers · {gradersActive} graders · {rulesActive} rules · {analyzersActive}
			analyzers
		</span>
	{/snippet}
	{#snippet children()}
		<div class="scp-body">
			<SessionSteeringSection {sessionId} {now} embedded />
			<AgentsSteerersBand steerers={steererEntries} {agents} {onJumpTo} embedded />
		</div>
	{/snippet}
</SectionCard>

<style>
	.scp-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-2, 8px);
	}

	.scp-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		text-align: right;
		white-space: normal;
	}
</style>
