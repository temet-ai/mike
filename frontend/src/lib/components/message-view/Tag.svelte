<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import type { WithElementRef } from '$lib/utils';
	import type { TagVariant } from './Tag.variants';
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';

	interface Props extends WithElementRef<HTMLAttributes<HTMLSpanElement>> {
		variant?: TagVariant;
	}

	let {
		ref = $bindable(null),
		class: className,
		variant = 'default',
		children,
		...restProps
	}: Props = $props();

	const BADGE_VARIANT: Record<NonNullable<TagVariant>, BadgeVariant> = {
		default: 'tag-default',
		group: 'tag-group',
		block: 'tag-block',
		hook: 'tag-green',
		boot: 'tag-blue',
		'event-info': 'tag-blue',
		'event-warn': 'tag-amber',
		catchall: 'tag-default'
	};
</script>

<Badge
	{...restProps}
	bind:ref
	class={className}
	variant={BADGE_VARIANT[variant ?? 'default']}
	data-slot="message-view-tag"
>
	{@render children?.()}
</Badge>
