<!--
@component
A skill as Claude reads it: SKILL.md and each reference file, one tab each, the text exactly as
stored. When `editing` is on, SKILL.md is an editable field bound to `draftText`.
-->
<script lang="ts">
	import * as Tabs from '$lib/components/ui/tabs';
	import Field from '$lib/components/ui/field/field.svelte';
	import type { ReferenceFile } from '$lib/api/skills';

	let {
		skillMd,
		references,
		editing = false,
		draftText = $bindable('')
	}: {
		skillMd: string;
		references: ReferenceFile[];
		editing?: boolean;
		draftText?: string;
	} = $props();

	let tab = $state('SKILL.md');
</script>

<div class="sfv" data-slot="skill-files-viewer">
	<Tabs.Root bind:value={tab}>
		<Tabs.List aria-label="Files in this skill">
			<Tabs.Trigger value="SKILL.md">SKILL.md</Tabs.Trigger>
			{#each references as file (file.name)}
				<Tabs.Trigger value={file.name}>references/{file.name}.md</Tabs.Trigger>
			{/each}
		</Tabs.List>
		<Tabs.Content value="SKILL.md">
			{#if editing}
				<Field
					label="SKILL.md"
					hint="What Claude reads when it loads the skill. Saving keeps the reviewer's text beside your edit."
					multiline
					rows={24}
					bind:value={draftText}
				/>
			{:else}
				<pre class="sfv-text">{skillMd}</pre>
			{/if}
		</Tabs.Content>
		{#each references as file (file.name)}
			<Tabs.Content value={file.name}>
				{#if file.when_to_read}
					<p class="sfv-when">Claude reads this when {file.when_to_read}</p>
				{/if}
				<pre class="sfv-text">{file.content}</pre>
			</Tabs.Content>
		{/each}
	</Tabs.Root>
</div>

<style>
	.sfv {
		min-width: 0;
	}
	.sfv-text {
		margin: 0;
		padding: var(--space-3);
		background: var(--surface-code);
		border: 1px solid var(--border-default);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.sfv-when {
		margin: 0 0 var(--space-2);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
</style>
