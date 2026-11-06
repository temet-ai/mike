<script lang="ts">
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';

	interface Props {
		label: string;
		isActive?: boolean;
		href?: string;
	}

	let { label, isActive = false, href }: Props = $props();

	const variant = $derived<BadgeVariant>(isActive ? 'identity-active' : 'identity');
</script>

<Badge {variant} {href} data-slot="class-chip">
	{label}
</Badge>
