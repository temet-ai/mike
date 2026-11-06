<!--
@component
The accepted lessons a skill draft was written from, each with its kind and the prompt it was
learned from, in full.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import type { SkillLesson } from '$lib/api/skills';

	let { lessons }: { lessons: SkillLesson[] } = $props();

	function lessonHref(id: number): string {
		return resolve(
			`/sessions/improvement/lessons/${id}` as `/sessions/improvement/lessons/${string}`
		);
	}
</script>

<ol class="sll" data-slot="skill-lesson-list">
	{#each lessons as lesson (lesson.id)}
		<li>
			<p class="sll-meta">
				<a href={lessonHref(lesson.id)}>Lesson {lesson.id}</a>, {lesson.kind}{lesson.archived_at
					? ', since archived'
					: ''}
			</p>
			<p class="sll-text">{lesson.text}</p>
			{#if lesson.user_prompt}
				<p class="sll-from">Learned from: {lesson.user_prompt}</p>
			{/if}
		</li>
	{:else}
		<li class="sll-none">The lessons behind this draft are no longer stored.</li>
	{/each}
</ol>

<style>
	.sll {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.sll p {
		margin: 0;
	}
	.sll-meta,
	.sll-from,
	.sll-none {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.sll-text {
		font-size: var(--text-sm);
		color: var(--text-1);
		white-space: pre-wrap;
	}
	.sll-from {
		white-space: pre-wrap;
	}
	.sll-meta a {
		color: var(--text-2);
		text-decoration: underline;
	}
</style>
