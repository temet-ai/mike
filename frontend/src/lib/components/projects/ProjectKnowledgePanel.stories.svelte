<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import ProjectKnowledgePanel from './ProjectKnowledgePanel.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	import type { KnowledgeReadResponse, KnowledgeSearchResponse } from '$lib/types';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';

	const PROJECT_PATH = 'projects/temet/temet-proxy';

	const SEARCH: KnowledgeSearchResponse = {
		project_uuid: '018f7b2a-0000-7000-8000-000000000007',
		retrieval_path: 'hybrid',
		items: [
			{
				reference: 'artifact:docs/decisions/embedding-model-choice.md',
				source_kind: 'artifact',
				title: 'Embedding model choice for project knowledge',
				status: 'current',
				path: 'docs/decisions/embedding-model-choice.md',
				updated_at: '2026-09-18T14:20:00Z',
				rank: 1,
				lexical_rank: 1,
				semantic_rank: 2,
				provenance: { collector: 'artifacts', revision: 3 }
			},
			{
				reference: 'lesson:501',
				source_kind: 'lesson',
				title: 'Never git reset --hard in a shared worktree',
				status: 'active',
				path: null,
				updated_at: '2026-07-15T10:00:00Z',
				rank: 2,
				semantic_rank: 1,
				provenance: { collector: 'lessons' }
			},
			{
				reference: 'turn:9001',
				source_kind: 'turn',
				title: 'Delete the stale worktree at .claude/worktrees/old-thing.',
				status: 'corrected',
				path: null,
				updated_at: '2026-07-15T10:05:00Z',
				rank: 3,
				lexical_rank: 4,
				provenance: { collector: 'turns', session_id: 'a1b2c3d4-e5f6-7890-abcd-ef1234567890' }
			}
		],
		has_more: false,
		next_offset: null,
		read_tool: 'read_project_knowledge',
		embedding: {
			coverage: { total: 42, ready: 40, pending: 2, failed: 0 },
			model: 'nomic-embed-text',
			fallback: null
		}
	};

	const CONTENT_HASH = 'a3f5c1d8e2b74096f1a0c3d5e7b9284617d0f3a5c8e1b4297d6f0a3c5e8b1d42';

	const READ_BODY = [
		'# Embedding model choice for project knowledge',
		'',
		'Decision: embed retained project knowledge locally with nomic-embed-text.',
		'',
		'- Retrieval is hybrid: lexical rank fused with semantic rank.',
		'- When the embedding backend is unreachable the search falls back to lexical only',
		'  and reports the reason, so the panel never silently degrades.',
		'- Remote embeddings stay off by default; captured text does not leave the machine.'
	].join('\n');

	const READ: KnowledgeReadResponse = {
		reference: SEARCH.items[0]!.reference,
		body: READ_BODY,
		title: SEARCH.items[0]!.title,
		path: 'docs/decisions/embedding-model-choice.md',
		status: 'current',
		kind: 'artifact',
		current: true,
		content_hash: CONTENT_HASH
	};

	const { Story } = defineMeta({
		title: 'Projects/ProjectKnowledgePanel',
		component: ProjectKnowledgePanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	args={{ projectPath: PROJECT_PATH }}
	loaders={[
		async () => {
			localStorage.clear();
			stubFetchJsonRouter((_method, url) => {
				if (url.includes('/api/knowledge/search')) return typed<KnowledgeSearchResponse>(SEARCH);
				if (url.includes('/api/knowledge/read')) return typed<KnowledgeReadResponse>(READ);
				return typed<Record<string, never>>({});
			});
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(CONTENT_HASH).toHaveLength(64);

		await waitFor(() => expect(canvas.getByText('3 results')).toBeInTheDocument());
		await expect(canvas.getByText('text + semantic')).toBeInTheDocument();
		await expect(canvas.getByText('40/42 semantic')).toBeInTheDocument();
		await expect(canvas.getByText('artifact')).toBeInTheDocument();
		await expect(canvas.getByText('lesson')).toBeInTheDocument();
		await expect(canvas.getByText('turn')).toBeInTheDocument();
		await expect(canvas.queryByRole('dialog')).toBeNull();

		await userEvent.click(
			canvas.getByRole('button', { name: `Read ${SEARCH.items[0]!.title}` })
		);

		const dialog = await canvas.findByRole('dialog', { name: 'Project knowledge source' });
		await waitFor(() => expect(dialog.querySelector('pre')?.textContent).toBe(READ_BODY));
		await expect(within(dialog).getByText('current')).toBeInTheDocument();

		const hash = within(dialog).getByText(CONTENT_HASH);
		await expect(hash.textContent).toBe(CONTENT_HASH);
		await expect(hash.textContent).toHaveLength(64);

		await userEvent.click(within(dialog).getByRole('button', { name: 'Close source' }));
		await waitFor(() => expect(canvas.queryByRole('dialog')).toBeNull());
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			{#snippet children()}
				<div class="w-[560px] bg-[var(--surface-bg)] p-6">
					<ProjectKnowledgePanel {...args} />
				</div>
			{/snippet}
		</CollapseStoreProvider>
	{/snippet}
</Story>
