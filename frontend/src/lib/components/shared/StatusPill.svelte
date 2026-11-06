<script lang="ts">
	import type { RequestStatus } from '$lib/types';
	import Tooltip from './Tooltip.svelte';
	import StreamingIndicator from './StreamingIndicator.svelte';
	import Badge from './Badge.svelte';
	import type { BadgeVariant } from './Badge.variants';

	let { status }: { status: RequestStatus } = $props();

	type Kind = 'complete' | 'pending' | 'streaming' | 'error' | 'unknown';

	function classify(v: RequestStatus): Kind {
		if (v === 'Complete') return 'complete';
		if (v === 'Streaming') return 'streaming';
		if (v === 'Pending') return 'pending';
		if (typeof v === 'object' && v !== null && 'Error' in v) return 'error';
		return 'unknown';
	}

	const variantByKind: Record<Kind, BadgeVariant> = {
		complete: 'pill-complete',
		pending: 'pill-pending',
		streaming: 'pill-streaming',
		error: 'pill-error',
		unknown: 'pill-unknown'
	};

	function tooltipText(v: RequestStatus, k: Kind): string {
		switch (k) {
			case 'complete':
				return 'Request finished, response stored. Click to inspect.';
			case 'streaming':
				return 'Response is still arriving — chunks streaming in via SSE.';
			case 'pending':
				return 'Request received, awaiting upstream response.';
			case 'error': {
				const err = typeof v === 'object' && v !== null && 'Error' in v ? v.Error : '';
				return err
					? `Upstream returned an error: ${err}. Click to inspect the captured error body.`
					: 'Upstream returned an error. Click to inspect the captured error body.';
			}
			default:
				return 'Status not recognised — backend may be on a newer schema.';
		}
	}

	const kind = $derived(classify(status));
	const label = $derived(typeof status === 'string' ? status : 'Error');
	const tip = $derived(tooltipText(status, kind));
</script>

<Tooltip text={tip} slug="request-status">
	<Badge variant={variantByKind[kind]} data-slot="status-pill" data-status={kind}>
		{#if kind === 'streaming' || kind === 'pending'}
			<StreamingIndicator label={tip} />
		{/if}
		{label}
	</Badge>
</Tooltip>
