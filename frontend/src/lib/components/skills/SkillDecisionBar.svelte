<!--
@component
The user's decisions on one skill draft. Every button is always present and is disabled when it
does not apply, so the bar never moves; the line under it says what the next promoter run will do.
-->
<script module lang="ts">
	export type SkillDecision = 'approve' | 'reject' | 'clear' | 'edit' | 'retire' | 'restore';
</script>

<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import type { DraftReview } from '$lib/api/skills';

	let {
		review,
		retired,
		editing,
		busy = null,
		outcome,
		error = null,
		onDecide
	}: {
		review: DraftReview | null;
		retired: boolean;
		editing: boolean;
		busy?: SkillDecision | null;
		outcome: string;
		error?: string | null;
		onDecide: (decision: SkillDecision) => void;
	} = $props();

	const locked = $derived(busy !== null || editing);
</script>

<div class="sdb" data-slot="skill-decision-bar">
	<div class="sdb-buttons" role="group" aria-label="Your decision on this skill">
		<Button
			variant="default"
			disabled={locked || review === 'approved' || retired}
			onclick={() => onDecide('approve')}>Approve</Button
		>
		<Button
			variant="outline"
			disabled={locked || review === 'rejected' || retired}
			onclick={() => onDecide('reject')}>Reject</Button
		>
		<Button variant="ghost" disabled={locked || review === null} onclick={() => onDecide('clear')}
			>Undo decision</Button
		>
		<Button variant="outline" disabled={locked || retired} onclick={() => onDecide('edit')}
			>Edit SKILL.md</Button
		>
		<Button variant="outline" disabled={locked || retired} onclick={() => onDecide('retire')}
			>Retire</Button
		>
		<Button variant="outline" disabled={locked || !retired} onclick={() => onDecide('restore')}
			>Restore</Button
		>
	</div>
	<p class="sdb-outcome" aria-live="polite" data-error={error !== null}>{error ?? outcome}</p>
</div>

<style>
	.sdb {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.sdb-buttons {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.sdb-outcome {
		margin: 0;
		min-height: 2lh;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.sdb-outcome[data-error='true'] {
		color: var(--status-danger-fg);
	}
</style>
