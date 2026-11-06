<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import CodeBlock from './CodeBlock.svelte';

	const { Story } = defineMeta({
		title: 'Components/MessageView/CodeBlock',
		component: CodeBlock,
		tags: ['autodocs']
	});
</script>

<Story
	name="Reminder"
	args={{
		lang: 'reminder',
		code: 'SessionStart:startup hook success: OK'
	}}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-card); max-width: 680px;">
			<CodeBlock {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Markdown"
	args={{
		lang: 'markdown',
		code: '# proxy/CLAUDE.md\n\n## Project context\nproxy/ is the edge layer that sits in front of the upstream model providers.'
	}}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-card); max-width: 680px;">
			<CodeBlock {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="NotCopyable"
	args={{
		lang: 'json',
		code: '{\n  "model": "claude-opus-4-7",\n  "max_tokens": 4096\n}',
		copyable: false
	}}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-card); max-width: 680px;">
			<CodeBlock {...args} />
		</div>
	{/snippet}
</Story>
