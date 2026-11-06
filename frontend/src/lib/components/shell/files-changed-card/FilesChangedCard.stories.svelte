<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import FilesChangedCard from './FilesChangedCard.svelte';

	const CHANGED_FILES = [
		'crates/mike/src/store/persistence/messages.rs',
		'crates/mike/src/store/persistence/judge.rs',
		'crates/mike/src/store/persistence/mailbox.rs',
		'crates/mike/src/api/handlers.rs',
		'crates/mike/src/api/manifest.rs',
		'crates/mike/src/proxy/handler.rs',
		'crates/mike-mcp/src/tools.rs',
		'frontend/src/lib/components/shell/files-changed-card/FilesChangedCard.svelte',
		'frontend/src/lib/components/shell/SessionOverview.svelte',
		'frontend/src/lib/components/shell/SessionMetrics.svelte',
		'frontend/src/lib/api/sessions.ts',
		'frontend/src/lib/stores/cache/sessionDetailStore.svelte.ts',
		'frontend/src/app.css',
		'migrations/0104_session_summary_v.sql',
		'migrations/0105_session_token_usage.sql',
		'Cargo.toml',
		'docs/ARCHITECTURE.md',
		'docs/REQUEST_FLOW.md',
		'scripts/start-proxy.sh'
	];

	const { Story } = defineMeta({
		title: 'Components/Shell/FilesChangedCard',
		component: FilesChangedCard,
		tags: ['autodocs'],
		argTypes: {
			files: { control: 'object' },
			loading: { control: 'boolean' },
			error: { control: 'text' }
		}
	});

	const LONG_DIRECTORY =
		'private/tmp/claude-501/-Users-vimes-projects-temet-temet-proxy/fc86185d-51b2-4a7b-a3d0-2502b7a7ac69/scratchpad';
	const STORY_FILES = [...CHANGED_FILES, `${LONG_DIRECTORY}/shoot-both.cjs`];
</script>

<Story
	name="Default"
	tags={['interactive']}
	args={{ files: STORY_FILES, loading: false, error: null, onRetry: () => {} }}
	play={async ({ canvasElement }) => {
		const paths = () =>
			Array.from(canvasElement.querySelectorAll<HTMLElement>('[data-path]')).map(
				(row) => row.dataset.path ?? ''
			);
		const expandAll = canvasElement.querySelector<HTMLButtonElement>(
			'[aria-label="Expand all folders"]'
		);
		const collapseAll = canvasElement.querySelector<HTMLButtonElement>(
			'[aria-label="Collapse all folders"]'
		);
		if (!expandAll || !collapseAll) throw new Error('Expected the bulk expand/collapse controls');

		expect(canvasElement.querySelector('.fc-file-tree')).not.toBeNull();
		expect(getComputedStyle(canvasElement.querySelector('.fc-file-tree')!).overflowY).toBe(
			'visible'
		);
		expect(paths().length).toBeLessThan(STORY_FILES.length);
		expect(canvasElement.querySelectorAll('[data-folder-icon="closed"]').length).toBeGreaterThan(0);

		await userEvent.click(expandAll);
		await waitFor(() => expect(paths()).toHaveLength(STORY_FILES.length));

		const fileRows = Array.from(canvasElement.querySelectorAll<HTMLElement>('[data-path]'));
		const renderedPaths = paths();
		for (const path of STORY_FILES) expect(renderedPaths).toContain(path);
		for (const row of fileRows) expect(row.querySelector('svg.fc-file-icon')).not.toBeNull();

		expect(canvasElement.querySelector('.fc-tree-count')).toBeNull();
		expect(canvasElement.querySelectorAll('.fc-tree-separator').length).toBeGreaterThan(0);
		expect(canvasElement.querySelector('.fc-tree-row--connected-before')).not.toBeNull();
		expect(canvasElement.querySelector('.fc-tree-row--connected-after')).not.toBeNull();
		expect(canvasElement.textContent).toContain('private/tmp/.../scratchpad');

		const longDirectory = canvasElement.querySelector<HTMLElement>(
			`[data-folder-path="${LONG_DIRECTORY}"]`
		);
		expect(longDirectory?.getAttribute('title')).toBe(LONG_DIRECTORY);

		const folder = canvasElement.querySelector<HTMLButtonElement>(
			'[data-folder-path="frontend/src/lib"]'
		);
		if (!folder) throw new Error('Expected frontend/src/lib folder row');

		expect(folder.getAttribute('aria-expanded')).toBe('true');
		expect(folder.dataset.folderIcon).toBe('open');
		await userEvent.click(folder);
		await waitFor(() => expect(folder.getAttribute('aria-expanded')).toBe('false'));
		expect(folder.dataset.folderIcon).toBe('closed');
		expect(
			canvasElement.querySelector('[data-path="frontend/src/lib/api/sessions.ts"]')
		).toBeNull();

		await userEvent.click(folder);
		await waitFor(() => expect(folder.getAttribute('aria-expanded')).toBe('true'));

		await userEvent.click(collapseAll);
		await waitFor(() => expect(paths()).toEqual(['Cargo.toml']));
		expect(canvasElement.querySelectorAll('.fc-directory-button').length).toBeGreaterThan(0);

		await userEvent.click(expandAll);
		await waitFor(() => expect(paths()).toHaveLength(STORY_FILES.length));
		expandAll.blur();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 360px">
			<FilesChangedCard {...args} />
		</div>
	{/snippet}
</Story>
