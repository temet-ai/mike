<!--
@component
The state of one learned skill folder, in the words the Skills page uses, as a Badge.
-->
<script lang="ts">
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
	import { STATE_TEXT, type SkillState } from '$lib/api/skills';

	let { state }: { state: SkillState } = $props();

	const VARIANT: Record<SkillState, BadgeVariant> = {
		waiting_on_you: 'chip-warning',
		shipped: 'chip-active',
		ready_to_ship: 'tag-green',
		approved_not_yet_shipped: 'tag-green',
		being_reviewed: 'tag-blue',
		waiting_for_reviewer: 'tag-blue',
		failed_review: 'chip-muted',
		rejected_by_you: 'chip-muted',
		retired_by_you: 'chip-muted',
		retired_unloaded: 'chip-muted',
		retired_corrected: 'chip-muted',
		removed: 'chip-muted',
		generated_only: 'tag-default'
	};
</script>

<Badge variant={VARIANT[state]} data-state={state}>{STATE_TEXT[state]}</Badge>
