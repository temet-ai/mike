<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import ToolUsesPanel from './ToolUsesPanel.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	import type { ToolUseDto, ToolUsesResponse } from '$lib/types';
	import 'augmented-ui/augmented-ui.min.css';

	const STORY_SESSION_ID = 'storybook-session-1';
	const STATIC_NOW = new Date('2026-04-17T12:10:00Z').getTime();

	function buildToolUse(
		overrides: Partial<ToolUseDto> & { tool_name: string; input: unknown }
	): ToolUseDto {
		return {
			request_id: 101,
			session_id: STORY_SESSION_ID,
			created_at: '2026-04-17T12:00:00Z',
			tool_id: 'toolu_01Abc',
			file_path: null,
			ord: 0,
			...overrides
		};
	}

	const TOOL_ROWS: ToolUseDto[] = [
		buildToolUse({
			tool_id: 'toolu_01Abc',
			tool_name: 'Read',
			input: { file_path: '/Users/dev/project/src/auth/jwt.ts' },
			file_path: '/Users/dev/project/src/auth/jwt.ts',
			created_at: '2026-04-17T12:00:00Z',
			ord: 0
		}),
		buildToolUse({
			request_id: 101,
			tool_id: 'toolu_02Def',
			tool_name: 'Edit',
			input: {
				file_path: '/Users/dev/project/src/auth/jwt.ts',
				old_string: 'const token = jwt.sign(payload, secret);',
				new_string: 'const token = jwt.sign(payload, secret, { expiresIn: "1h" });'
			},
			file_path: '/Users/dev/project/src/auth/jwt.ts',
			created_at: '2026-04-17T12:01:00Z',
			ord: 1
		}),
		buildToolUse({
			request_id: 102,
			tool_id: 'toolu_03Ghi',
			tool_name: 'Bash',
			input: {
				command: 'cargo test --all -- --test-threads=1 2>&1 | tail -20',
				description: 'Run the full test suite'
			},
			file_path: null,
			created_at: '2026-04-17T12:02:00Z',
			ord: 0
		}),
		buildToolUse({
			request_id: 102,
			tool_id: 'toolu_04Jkl',
			tool_name: 'Write',
			input: {
				file_path: '/Users/dev/project/src/auth/token_store.ts',
				content:
					'export interface TokenStore {\n  set(key: string, value: string): void;\n  get(key: string): string | null;\n}'
			},
			file_path: '/Users/dev/project/src/auth/token_store.ts',
			created_at: '2026-04-17T12:03:00Z',
			ord: 1
		}),
		buildToolUse({
			request_id: 103,
			tool_id: 'toolu_05Mno',
			tool_name: 'Task',
			input: {
				description: 'Implement the JWT refresh flow and write tests for token expiry',
				subagent_type: 'backend-specialist'
			},
			file_path: null,
			created_at: '2026-04-17T12:04:00Z',
			ord: 0
		})
	];

	const MANY_ROWS: ToolUseDto[] = Array.from({ length: 30 }, (_, i) =>
		buildToolUse({
			request_id: 100 + Math.floor(i / 3),
			tool_id: `toolu_${String(i).padStart(2, '0')}X`,
			tool_name: ['Read', 'Edit', 'Bash', 'Write', 'Task'][i % 5] as string,
			input:
				{ file_path: `/src/module${i}/index.ts`, command: `pnpm test --module ${i}` }[
					(i % 2 === 0 ? 'file_path' : 'command') as 'file_path' | 'command'
				] !== undefined
					? { file_path: `/src/module${i}/index.ts` }
					: { command: `pnpm test --module ${i}` },
			file_path: i % 2 === 0 ? `/src/module${i}/index.ts` : null,
			created_at: `2026-04-17T12:${String(Math.floor(i / 2)).padStart(2, '0')}:${String(i % 60).padStart(2, '0')}Z`,
			ord: i % 3
		})
	);

	function stubFetch(rows: ToolUseDto[], totalCount?: number): () => void {
		const response: ToolUsesResponse = {
			session_id: STORY_SESSION_ID,
			total_count: totalCount ?? rows.length,
			offset: 0,
			limit: 1000,
			rows
		};
		const orig = globalThis.fetch;
		globalThis.fetch = async () =>
			new Response(JSON.stringify(response), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		return () => {
			globalThis.fetch = orig;
		};
	}

	const NON_FILE_ROWS: ToolUseDto[] = [
		buildToolUse({
			request_id: 110,
			tool_id: 'toolu_nf01',
			tool_name: 'Bash',
			input: {
				command: 'cargo nextest run --all 2>&1 | tail -40',
				description: 'Run the full suite'
			},
			file_path: null,
			created_at: '2026-04-17T12:00:00Z',
			ord: 0
		}),
		buildToolUse({
			request_id: 110,
			tool_id: 'toolu_nf02',
			tool_name: 'Task',
			input: {
				description:
					'Audit the storybook coverage matrix and report unstoried state deltas across every component domain',
				subagent_type: 'frontend-developer'
			},
			file_path: null,
			created_at: '2026-04-17T12:01:00Z',
			ord: 1
		}),
		buildToolUse({
			request_id: 111,
			tool_id: 'toolu_nf03',
			tool_name: 'Grep',
			input: { pattern: 'line-clamp|truncate', glob: '*.svelte' },
			file_path: null,
			created_at: '2026-04-17T12:02:00Z',
			ord: 0
		})
	];

	const { Story } = defineMeta({
		title: 'Shell/ToolUsesPanel',
		component: ToolUsesPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			const restore = stubFetch(TOOL_ROWS);
			return { restore };
		}
	]}
>
	{#snippet template()}
		<div style="width: 500px; background: var(--surface-bg); padding: 8px;">
			<ToolUsesPanel sessionId={STORY_SESSION_ID} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="Empty"
	loaders={[
		async () => {
			const restore = stubFetch([]);
			return { restore };
		}
	]}
>
	{#snippet template()}
		<div style="width: 500px; background: var(--surface-bg); padding: 8px;">
			<ToolUsesPanel sessionId={STORY_SESSION_ID} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="Loading"
	loaders={[
		async () => {
			const orig = globalThis.fetch;
			globalThis.fetch = async () => new Promise(() => {});
			return {
				restore: () => {
					globalThis.fetch = orig;
				}
			};
		}
	]}
>
	{#snippet template()}
		<div style="width: 500px; background: var(--surface-bg); padding: 8px;">
			<ToolUsesPanel sessionId={STORY_SESSION_ID} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	loaders={[
		async () => {
			const orig = globalThis.fetch;
			globalThis.fetch = async () =>
				new Response(JSON.stringify({ error: 'Session not found' }), {
					status: 404,
					headers: { 'content-type': 'application/json' }
				});
			return {
				restore: () => {
					globalThis.fetch = orig;
				}
			};
		}
	]}
>
	{#snippet template()}
		<div style="width: 500px; background: var(--surface-bg); padding: 8px;">
			<ToolUsesPanel sessionId={STORY_SESSION_ID} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="ManyRows"
	loaders={[
		async () => {
			const restore = stubFetch(MANY_ROWS);
			return { restore };
		}
	]}
>
	{#snippet template()}
		<div
			style="width: 500px; height: 600px; background: var(--surface-bg); overflow: auto; padding: 8px;"
		>
			<ToolUsesPanel sessionId={STORY_SESSION_ID} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="NonFileTools"
	loaders={[
		async () => {
			const restore = stubFetch(NON_FILE_ROWS);
			return { restore };
		}
	]}
>
	{#snippet template()}
		<div style="width: 500px; background: var(--surface-bg); padding: 8px;">
			<ToolUsesPanel sessionId={STORY_SESSION_ID} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="PaginatedTail"
	loaders={[
		async () => {
			const restore = stubFetch(TOOL_ROWS, 842);
			return { restore };
		}
	]}
>
	{#snippet template()}
		<div style="width: 500px; background: var(--surface-bg); padding: 8px;">
			<ToolUsesPanel sessionId={STORY_SESSION_ID} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="ClipboardUnavailable"
	loaders={[
		async () => {
			const origFetch = globalThis.fetch;
			const stubResponse: ToolUsesResponse = {
				session_id: STORY_SESSION_ID,
				total_count: TOOL_ROWS.length,
				offset: 0,
				limit: 1000,
				rows: TOOL_ROWS
			};
			globalThis.fetch = async () =>
				new Response(JSON.stringify(stubResponse), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			const clipboardDesc = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
			try {
				Object.defineProperty(navigator, 'clipboard', {
					configurable: true,
					value: undefined
				});
			} catch {}
			return {
				restore: () => {
					globalThis.fetch = origFetch;
					if (clipboardDesc) {
						Object.defineProperty(navigator, 'clipboard', clipboardDesc);
					}
				}
			};
		}
	]}
>
	{#snippet template()}
		<div style="width: 500px; background: var(--surface-bg); padding: 8px;">
			<ToolUsesPanel sessionId={STORY_SESSION_ID} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="RowExpanded"
	loaders={[
		async () => {
			const restore = stubFetch(TOOL_ROWS);
			return { restore };
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const section = await waitFor(() => {
			const el = canvas.getByRole('button', { name: /tool uses/i });
			return el as HTMLButtonElement;
		});
		if (section.getAttribute('aria-expanded') !== 'true') {
			await userEvent.click(section);
		}
		const toggle = await waitFor(() => {
			const el = canvasElement.querySelector('button[aria-label="Expand Read tool call"]');
			if (!el) throw new Error('row toggle not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(toggle);
		const rawToggle = await waitFor(() => {
			const el = canvasElement.querySelector('button.tool-card__raw-toggle');
			if (!el) throw new Error('raw toggle not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(rawToggle);
		const input = await waitFor(
			() => {
				const el = canvasElement.querySelector('[aria-label="Full tool input JSON"]');
				if (!el) throw new Error('input JSON container not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(input).toBeInTheDocument();
		await expect(
			canvasElement.querySelector('button[aria-label="Collapse Read tool call"]')
		).toBeInTheDocument();
		void canvas;
	}}
>
	{#snippet template()}
		<CollapseStoreProvider>
			{#snippet children()}
				<div style="width: 500px; background: var(--surface-bg); padding: 8px;">
					<ToolUsesPanel sessionId={STORY_SESSION_ID} now={STATIC_NOW} />
				</div>
			{/snippet}
		</CollapseStoreProvider>
	{/snippet}
</Story>
