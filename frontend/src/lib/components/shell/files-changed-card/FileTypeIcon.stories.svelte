<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import FileTypeIcon from './FileTypeIcon.svelte';

	const EXAMPLE_PATHS = [
		'crates/mike/src/lib.rs',
		'frontend/src/lib/api/sessions.ts',
		'frontend/src/App.svelte',
		'migrations/0104_session_summary_v.sql',
		'docs/ARCHITECTURE.md',
		'Cargo.toml',
		'scripts/start-proxy.unknown'
	];

	const { Story } = defineMeta({
		title: 'Components/Shell/FileTypeIcon',
		component: FileTypeIcon,
		tags: ['autodocs'],
		argTypes: {
			path: { control: 'text' },
			size: { control: 'number' },
			class: { control: 'text' }
		}
	});
</script>

<Story
	name="Default"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const svgs = Array.from(canvasElement.querySelectorAll('svg'));
		expect(svgs.length).toBe(EXAMPLE_PATHS.length);
		for (const svg of svgs) {
			expect(svg).not.toBeNull();
		}
	}}
>
	{#snippet template()}
		<div style="display: flex; gap: 8px; align-items: center; flex-wrap: wrap; padding: 8px;">
			{#each EXAMPLE_PATHS as path}
				<FileTypeIcon {path} size={20} />
			{/each}
		</div>
	{/snippet}
</Story>
