<script lang="ts">
	import SessionRow from '../SessionRow.svelte';
	import type { DetailMetrics, SessionMatchInfo } from '../sessionRowTypes';
	import type { SessionSummary, ProxyEvent } from '$lib/types';

	interface Props {
		session: SessionSummary;
		now: number;
		detail: DetailMetrics | undefined;
		matchInfo?: SessionMatchInfo | null;
	}

	let { session: initialSession, now, detail, matchInfo = null }: Props = $props();

	// svelte-ignore state_referenced_locally
	let session = $state(initialSession);
	let matchExpanded = $state(false);

	function latestWsEventForSession(_sessionId: string): ProxyEvent | null {
		return null;
	}

	function onTogglePin(): void {
		session = { ...session, pinned: !(session.pinned === true) };
	}

	function onToggleMatchExpand(): void {
		matchExpanded = !matchExpanded;
	}
</script>

<SessionRow
	{session}
	{now}
	{detail}
	{latestWsEventForSession}
	onHoverIntent={() => {}}
	onHoverCancel={() => {}}
	{onTogglePin}
	{matchInfo}
	{matchExpanded}
	{onToggleMatchExpand}
/>
