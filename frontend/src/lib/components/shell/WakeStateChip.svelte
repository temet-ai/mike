<script lang="ts">
	import Badge from '$lib/components/shared/Badge.svelte';
	import { wakeLabel } from '$lib/api/steerQuestions';
	import type { WakeFields } from '$lib/api/steerQuestions';

	let { fields }: { fields: WakeFields } = $props();

	const label = $derived(wakeLabel(fields));
</script>

{#if label}
	<span
		class="inline-flex items-center gap-1"
		data-wake-state={fields.wake_state ?? fields.delivery_channel}
	>
		<Badge variant="chip-muted" data-wake-axis="channel">{fields.delivery_channel}</Badge>
		<Badge
			variant={fields.wake_state === 'pushed' ? 'chip-active' : 'chip-muted'}
			data-wake-axis="state">{label}</Badge
		>
	</span>
{/if}
