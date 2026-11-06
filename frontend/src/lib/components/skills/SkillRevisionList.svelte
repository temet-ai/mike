<!--
@component
Every draft of one skill folder, newest first, each linking to its own page.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import type { SkillRevision } from '$lib/api/skills';

	let { revisions, current }: { revisions: SkillRevision[]; current: number } = $props();

	const STATUS: Record<SkillRevision['status'], string> = {
		draft: 'waiting for the reviewer',
		evaluating: 'being reviewed',
		accepted: 'passed review',
		rejected: 'did not pass review'
	};

	function href(id: number): string {
		return resolve(`/skills/${id}` as `/skills/${string}`);
	}
</script>

<ol class="srl" data-slot="skill-revision-list">
	{#each revisions as r (r.id)}
		<li aria-current={r.id === current ? 'page' : undefined}>
			<a href={href(r.id)}>Revision {r.revision} at lesson {r.lesson_watermark}</a>,
			{STATUS[r.status]}{r.review ? `, ${r.review} by you` : ''}, written {new Date(
				r.created_at
			).toLocaleDateString()}
		</li>
	{/each}
</ol>

<style>
	.srl {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.srl li[aria-current='page'] {
		color: var(--text-strong);
	}
</style>
