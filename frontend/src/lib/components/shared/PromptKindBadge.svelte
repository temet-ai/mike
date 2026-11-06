<script module lang="ts">
	import { KIND_CLASS, KIND_LABEL, KIND_TIP, isKnownKind } from './promptKind';
	import type { BadgeVariant } from './Badge.variants';

	const CLASS_TO_VARIANT: Record<string, BadgeVariant> = {
		'badge-nx badge-active': 'chip-active',
		'badge-nx badge-muted': 'chip-muted'
	};
</script>

<script lang="ts">
	import Tooltip from './Tooltip.svelte';
	import Badge from './Badge.svelte';

	let { kind }: { kind: string | null | undefined } = $props();

	const known = $derived(kind != null && isKnownKind(kind) ? kind : null);
	const variant = $derived<BadgeVariant>(
		known ? (CLASS_TO_VARIANT[KIND_CLASS[known]] ?? 'chip-muted') : 'chip-muted'
	);
	const label = $derived(known ? KIND_LABEL[known] : (kind ?? ''));
	const tip = $derived(
		known ? KIND_TIP[known] : 'Prompt kind not recognised — backend may be on a newer schema.'
	);
</script>

{#if kind != null}
	<Tooltip text={tip}>
		<Badge {variant} data-prompt-kind={kind}>{label}</Badge>
	</Tooltip>
{/if}
