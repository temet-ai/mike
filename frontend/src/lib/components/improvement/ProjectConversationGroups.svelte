<!--
@component
Items waiting on you, grouped by project and then by the conversation they came from: a project
band (name, full path on hover, count), then a band per conversation linking to it, then each
item. Shared by the Lessons and Nudges lists so both read the same way.
-->
<script lang="ts" generics="T extends ConversationItem">
	import type { Snippet } from 'svelte';
	import { groupByProjectAndConversation } from './groupByConversation';
	import type { ConversationItem } from './groupByConversation';

	interface Props {
		items: readonly T[];
		key: (item: T) => string | number;
		one: string;
		many: string;
		item: Snippet<[T]>;
	}

	const { items, key, one, many, item }: Props = $props();

	const groups = $derived(groupByProjectAndConversation(items));

	function count(n: number): string {
		return `${n} ${n === 1 ? one : many}`;
	}
</script>

{#each groups as project (project.projectLabel)}
	<div class="band-hd band-hd--static pcg-project" data-slot="project-group">
		<h3 class="machine pcg-project-name" title={project.projectPath ?? undefined}>
			{project.projectLabel}
		</h3>
		<span class="band-hd__meta"
			>{project.conversations.length}
			{project.conversations.length === 1 ? 'conversation' : 'conversations'} · {count(
				project.count
			)}</span
		>
	</div>
	{#each project.conversations as conversation (conversation.href)}
		<div class="band-hd band-hd--static pcg-conv" data-slot="conversation-group">
			<h4 class="pcg-conv-name">
				<a class="pcg-link" href={conversation.href}>{conversation.title}</a>
			</h4>
			<span class="band-hd__meta">{count(conversation.items.length)}</span>
		</div>
		{#each conversation.items as entry (key(entry))}
			<div class="pcg-item">{@render item(entry)}</div>
		{/each}
	{/each}
{/each}

<style>
	.pcg-project {
		background: var(--surface-2);
	}
	.pcg-project:hover {
		background: var(--surface-2);
	}
	.pcg-project-name {
		margin: 0;
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		color: var(--text-1);
	}
	.pcg-conv {
		padding-left: calc(var(--space-4) + 10px + var(--space-2));
	}
	.pcg-conv-name {
		margin: 0;
		min-width: 0;
		font-size: var(--text-sm);
		font-weight: var(--weight-body);
		overflow-wrap: anywhere;
	}
	.pcg-link {
		color: var(--interactive-active-fg);
		text-underline-offset: var(--space-1);
	}
	.pcg-link:hover {
		text-decoration: underline;
	}
	.pcg-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.pcg-item {
		padding-left: var(--space-5);
	}
</style>
