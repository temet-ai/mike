<!--
@component
The Lessons panel: one frame holding a lead line that says what Approve and Reject do, then the
lessons grouped by project (alphabetical, "No project" last) and by the conversation each came
from, newest first. A decided lesson stays in its place; `decided` says how each was decided.
-->
<script lang="ts">
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import LessonCandidateCard from './LessonCandidateCard.svelte';
	import ProjectConversationGroups from './ProjectConversationGroups.svelte';
	import type { LessonCandidate, RejectReason } from '$lib/api/improvement';

	interface Props {
		candidates: LessonCandidate[];
		error: string | null;
		decided?: ReadonlyMap<number, 'approved' | 'rejected'>;
		detailHrefFor: (id: number) => string;
		onApprove: (id: number) => Promise<void>;
		onReject: (id: number, reason: RejectReason, note?: string) => Promise<void>;
		onRetry: () => void;
	}

	const {
		candidates,
		error,
		decided = new Map(),
		detailHrefFor,
		onApprove,
		onReject,
		onRetry
	}: Props = $props();

	const newestFirst = $derived(
		[...candidates].sort((a, b) => b.createdAt.localeCompare(a.createdAt))
	);

	let busyId = $state<number | null>(null);

	async function approve(id: number): Promise<void> {
		busyId = id;
		try {
			await onApprove(id);
		} finally {
			busyId = null;
		}
	}

	async function reject(id: number, reason: RejectReason, note?: string): Promise<void> {
		busyId = id;
		try {
			await onReject(id, reason, note);
		} finally {
			busyId = null;
		}
	}
</script>

<section
	class="lcl edge mech-group"
	aria-label="Lessons waiting on you"
	data-slot="lesson-candidate-list"
>
	<div class="band-body lcl-lead" data-testid="lesson-action-effect">
		Approve sends a rule to later sessions of its project, starting with their next message. Reject
		keeps it from ever being sent, and saves the reason you pick.
	</div>
	{#if error}
		<div class="band-body">
			<LoadErrorBanner title="Could not load lesson candidates." detail={error} {onRetry} />
		</div>
	{:else if candidates.length === 0}
		<div class="band-body lcl-empty">No lessons are waiting on you.</div>
	{:else}
		<ProjectConversationGroups items={newestFirst} key={(c) => c.id} one="lesson" many="lessons">
			{#snippet item(candidate)}
				<LessonCandidateCard
					{candidate}
					showSession={false}
					detailHref={detailHrefFor(candidate.id)}
					decision={decided.get(candidate.id) ?? 'waiting'}
					busy={busyId !== null && busyId !== candidate.id}
					onApprove={() => approve(candidate.id)}
					onReject={(reason, note) => reject(candidate.id, reason, note)}
				/>
			{/snippet}
		</ProjectConversationGroups>
	{/if}
</section>

<style>
	.lcl {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.lcl-lead,
	.lcl-empty {
		padding-top: var(--space-3);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
</style>
