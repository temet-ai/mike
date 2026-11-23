<script lang="ts">
	import { resolve } from '$app/paths';

	interface Props {
		id: number | null;
		title: string | null;
		project: string;
	}

	const { id, title, project }: Props = $props();
	const label = $derived(title ?? 'Subject removed');
</script>

{#if id !== null}
	<a
		class="subject-link"
		href={`${resolve('/memory/subjects/[id]', { id: String(id) })}?project=${encodeURIComponent(project)}`}
	>
		{label}
	</a>
{:else}
	<span class="subject-link subject-link--gone">{label}</span>
{/if}

<style>
	.subject-link {
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	a.subject-link:hover {
		color: var(--text-strong);
		text-decoration: underline;
	}
	a.subject-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}
	.subject-link--gone {
		color: var(--text-3);
	}
</style>
