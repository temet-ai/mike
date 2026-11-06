<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { setContext } from 'svelte';
	import { userEvent, within, expect, waitFor, fn } from 'storybook/test';
	import Page from './+page.svelte';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import { __setRetryDelaysForTesting, __resetRetryDelaysForTesting } from '$lib/api';
	import type { ToolUsesFeedRow, ToolUsesFeedResponse, ToolUseFacets } from '$lib/types';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';

	function buildRow(overrides: Partial<ToolUsesFeedRow> & { use_id: number }): ToolUsesFeedRow {
		return {
			use_id: overrides.use_id,
			wire_tool_use_id: `tool_use_${overrides.use_id}`,
			request_id: overrides.request_id ?? 1000 + overrides.use_id,
			session_id: overrides.session_id ?? '01J9X4QF7H8K2N3P5R6T7V8W9A',
			project: overrides.project !== undefined ? overrides.project : '/Users/dev/temet-proxy',
			tool_name: overrides.tool_name ?? 'Bash',
			created_at: overrides.created_at ?? '2026-06-17T10:00:00Z',
			ord: overrides.ord ?? overrides.use_id,
			input: overrides.input ?? { command: 'ls -la' },
			file_path: overrides.file_path ?? null,
			is_error: overrides.is_error,
			result_id: overrides.result_id,
			error_content: overrides.error_content,
			error_content_bytes: overrides.error_content_bytes,
			error_content_sha256: overrides.error_content_sha256
		};
	}

	function buildFeedResponse(rows: ToolUsesFeedRow[], total?: number): ToolUsesFeedResponse {
		return {
			total_count: total ?? rows.length,
			total_count_capped: false,
			offset: 0,
			limit: 200,
			next_cursor:
				total !== undefined && total > rows.length ? `1761596336615173.${rows.length}` : null,
			rows
		};
	}

	const DEFAULT_FACETS: ToolUseFacets = {
		projects: ['/Users/dev/temet-proxy', '/Users/dev/other-project'],
		tool_names: ['Bash', 'Read', 'Edit', 'Write', 'Task']
	};

	const MIXED_ROWS: ToolUsesFeedRow[] = [
		buildRow({
			use_id: 1,
			tool_name: 'Bash',
			input: { command: 'cargo nextest run --all' },
			project: '/Users/dev/temet-proxy',
			session_id: '01J9X4QF7H8K2N3P5R6T7V8W9A',
			created_at: '2026-06-17T10:09:00Z'
		}),
		buildRow({
			use_id: 2,
			tool_name: 'Task',
			input: { description: 'Investigate failing integration tests' },
			project: '/Users/dev/other-project',
			session_id: '01J9X4QF7H8K2N3P5R6T7V8W9B',
			created_at: '2026-06-17T10:08:00Z'
		}),
		buildRow({
			use_id: 3,
			tool_name: 'Edit',
			file_path: '/Users/dev/temet-proxy/frontend/app.css',
			input: {
				file_path: '/Users/dev/temet-proxy/frontend/app.css',
				old_string: 'old',
				new_string: 'new'
			},
			project: '/Users/dev/temet-proxy',
			session_id: '01J9X4QF7H8K2N3P5R6T7V8W9A',
			is_error: true,
			result_id: 501,
			error_content: 'String to replace not found in file.',
			error_content_bytes: 34,
			error_content_sha256: '3f9ac8d1e2b4567890abcdef1234567890abcdef1234567890abcdef123456',
			created_at: '2026-06-17T10:05:00Z'
		}),
		buildRow({
			use_id: 4,
			tool_name: 'Bash',
			input: { command: 'echo no project' },
			project: null,
			session_id: '01J9X4QF7H8K2N3P5R6T7V8W9C',
			created_at: '2026-06-17T10:02:00Z'
		}),
		buildRow({
			use_id: 5,
			tool_name: 'Read',
			input: { file_path: '/Users/dev/other-project/src/lib.rs' },
			project: '/Users/dev/other-project',
			session_id: '01J9X4QF7H8K2N3P5R6T7V8W9B',
			created_at: '2026-06-17T10:00:00Z'
		})
	];

	function stubToolUsesFetch(
		feedResponse: ToolUsesFeedResponse,
		facets: ToolUseFacets = DEFAULT_FACETS,
		loadMoreResponse?: ToolUsesFeedResponse
	): void {
		let callCount = 0;
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/tool-uses/facets')) {
				return new Response(JSON.stringify(facets), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/tool-uses')) {
				callCount++;
				const response = callCount > 1 && loadMoreResponse ? loadMoreResponse : feedResponse;
				return new Response(JSON.stringify(response), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	function stubToolUsesFetchError(): void {
		globalThis.fetch = async () =>
			new Response(JSON.stringify({ error: 'Service Unavailable' }), {
				status: 503,
				statusText: 'Service Unavailable',
				headers: { 'content-type': 'application/json' }
			});
	}

	const { Story } = defineMeta({
		title: 'Routes/Tools',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Loading"
	tags={['visual', 'data-driven']}
	loaders={[
		async () => {
			globalThis.fetch = () => new Promise(() => {});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const status = await waitFor(
			() => {
				const el = canvasElement.querySelector('.tu-status');
				if (!el) throw new Error('Loading status element not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(status).toHaveTextContent('Loading…');
		await captureStorySnapshot(canvasElement, 'page/ToolsLoading');
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Empty"
	tags={['visual', 'data-driven']}
	loaders={[
		async () => {
			stubToolUsesFetch(buildFeedResponse([]), { projects: [], tool_names: [] });
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const empty = await waitFor(
			() => {
				const el = canvasElement.querySelector('.filtered-empty-state');
				if (!el) throw new Error('Empty state element not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(empty).toHaveTextContent('No tool uses recorded');
		await captureStorySnapshot(canvasElement, 'page/ToolsEmpty');
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	tags={['visual', 'data-driven']}
	loaders={[
		async () => {
			__setRetryDelaysForTesting([0, 0, 0]);
			stubToolUsesFetchError();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const banner = await waitFor(
			() => {
				const el = canvasElement.querySelector('.tu-status--error');
				if (!el) throw new Error('Error status element not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(banner).toHaveTextContent("Couldn't load tool uses");
		await captureStorySnapshot(canvasElement, 'page/ToolsError');

		const retryBtn = within(canvasElement).getByRole('button', { name: 'Retry' });
		await userEvent.click(retryBtn);
		await waitFor(() => {
			const el = canvasElement.querySelector('.tu-status--error');
			if (!el) throw new Error('error banner did not reappear after retry');
		});

		__resetRetryDelaysForTesting();
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Default"
	tags={['visual', 'data-driven']}
	loaders={[
		async () => {
			stubToolUsesFetch(buildFeedResponse(MIXED_ROWS));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				const groups = canvasElement.querySelectorAll('.tpg-project');
				if (groups.length < 3) throw new Error('project groups not yet rendered');
			},
			{ timeout: 4000 }
		);

		const names = Array.from(canvasElement.querySelectorAll('.tpg-name')).map((el) =>
			el.textContent?.trim()
		);
		await expect(names).toEqual(['temet-proxy', 'other-project', 'Ungrouped']);

		const counts = Array.from(canvasElement.querySelectorAll('.tpg-counts')).map((el) =>
			el.textContent?.trim()
		);
		await expect(counts).toEqual(['2 calls, 1 failed', '2 calls', '1 call']);

		const failedBadges = canvasElement.querySelectorAll('.tpg-failed');
		await expect(failedBadges.length).toBe(1);

		await captureStorySnapshot(canvasElement, 'page/ToolsDefault');
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="ErrorsOnly"
	tags={['data-driven', 'interactive']}
	loaders={[
		async () => {
			const erroredOnly: ToolUsesFeedRow[] = [
				buildRow({
					use_id: 50,
					tool_name: 'Edit',
					file_path: '/Users/dev/temet-proxy/frontend/app.css',
					input: {
						file_path: '/Users/dev/temet-proxy/frontend/app.css',
						old_string: 'old',
						new_string: 'new'
					},
					is_error: true,
					result_id: 700,
					error_content: 'String to replace not found in file.',
					error_content_bytes: 34,
					error_content_sha256: 'deadbeef00112233445566778899aabbccddeeff00112233445566778899aa'
				})
			];
			const allRows: ToolUsesFeedRow[] = [
				...erroredOnly,
				buildRow({ use_id: 51, tool_name: 'Bash', input: { command: 'make test' } })
			];
			const onFetch = fn(async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/api/tool-uses/facets')) {
					return new Response(JSON.stringify(DEFAULT_FACETS), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				if (url.includes('/api/tool-uses')) {
					const scoped = url.includes('errors_only=true') ? erroredOnly : allRows;
					return new Response(JSON.stringify(buildFeedResponse(scoped)), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				return new Response('Not Found', { status: 404 });
			});
			globalThis.fetch = onFetch as unknown as typeof fetch;
			return { onFetch };
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools#errors'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement, loaded }) => {
		const onFetch = loaded.onFetch as ReturnType<typeof fn>;
		const toolUsesCalls = () =>
			onFetch.mock.calls.filter((args) => {
				const url = typeof args[0] === 'string' ? args[0] : String(args[0]);
				return url.includes('/api/tool-uses') && !url.includes('/facets');
			});

		const checkbox = await waitFor(
			() => {
				const el = canvasElement.querySelector<HTMLInputElement>('.tu-checkbox');
				if (!el) throw new Error('errors-only checkbox not yet rendered');
				return el;
			},
			{ timeout: 4000 }
		);
		await expect(checkbox.checked).toBe(true);

		await waitFor(
			() => {
				const hit = toolUsesCalls().some((args) => {
					const url = typeof args[0] === 'string' ? args[0] : String(args[0]);
					return url.includes('errors_only=true');
				});
				if (!hit) throw new Error('initial fetch with errors_only=true not yet observed');
			},
			{ timeout: 3000 }
		);

		const count = await waitFor(
			() => {
				const el = canvasElement.querySelector('.filter-scope-status__scope');
				if (!el) throw new Error('applied-scope line not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(count).toHaveTextContent('errors only');

		await expect(toolUsesCalls().length).toBe(1);

		await userEvent.click(checkbox);
		await waitFor(
			() => {
				if (toolUsesCalls().length < 2) throw new Error('toggle-off fetch not yet observed');
			},
			{ timeout: 3000 }
		);
		await expect(toolUsesCalls().length).toBe(2);
		const lastCallUrl = String(toolUsesCalls().at(-1)?.[0]);
		await expect(lastCallUrl).not.toContain('errors_only=true');
		await waitFor(() => expect(count).not.toHaveTextContent('errors only'));
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="RowExpand"
	tags={['interactive']}
	loaders={[
		async () => {
			stubToolUsesFetch(buildFeedResponse(MIXED_ROWS));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				if (canvasElement.querySelectorAll('.tpg-row__disclose').length < 5) {
					throw new Error('rows not yet rendered');
				}
			},
			{ timeout: 4000 }
		);

		const firstGroup = canvasElement.querySelector('.tpg-project') as HTMLElement;
		const [cleanDisclose, erroredDisclose] = Array.from(
			firstGroup.querySelectorAll('.tpg-row__disclose')
		) as HTMLButtonElement[];

		await userEvent.click(erroredDisclose!);
		const errorPre = await waitFor(() => {
			const el = firstGroup.querySelector('.tpg-pre--error');
			if (!el) throw new Error('error pre not yet rendered');
			return el as HTMLElement;
		});
		await expect(errorPre.textContent).toBe('String to replace not found in file.');
		const errorRowLabels = Array.from(firstGroup.querySelectorAll('.tpg-section__label')).map(
			(el) => el.textContent
		);
		await expect(errorRowLabels).toEqual(['Error', 'Input']);

		await userEvent.click(cleanDisclose!);
		await waitFor(() => {
			if (firstGroup.querySelectorAll('.tpg-pre').length < 3) {
				throw new Error('clean row input not yet rendered');
			}
		});
		await expect(firstGroup.querySelectorAll('.tpg-pre--error').length).toBe(1);
		await expect(firstGroup.querySelectorAll('.tpg-section__label').length).toBe(2);
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="ProjectCollapseToggle"
	tags={['interactive']}
	loaders={[
		async () => {
			stubToolUsesFetch(buildFeedResponse(MIXED_ROWS));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const firstProject = await waitFor(
			() => {
				const el = canvasElement.querySelector<HTMLElement>('.tpg-project');
				if (!el) throw new Error('project group not yet rendered');
				return el;
			},
			{ timeout: 4000 }
		);
		const toggle = firstProject.querySelector<HTMLButtonElement>('.tpg-toggle')!;
		await expect(toggle).toBeTruthy();
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');
		await expect(firstProject.querySelector('.tpg-table')).toBeTruthy();

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'false'));
		await waitFor(() => expect(firstProject.querySelector('.tpg-table')).toBeNull());

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));
		await expect(firstProject.querySelector('.tpg-table')).toBeTruthy();
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="CollapseStoreContext"
	tags={['interactive', 'data-driven']}
	loaders={[
		async () => {
			stubToolUsesFetch(buildFeedResponse(MIXED_ROWS));
			(globalThis as Record<string, unknown>).__toolsCollapseToggleSpy = undefined;
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const toggle = await waitFor(
			() => {
				const el = canvasElement.querySelector<HTMLButtonElement>('.tpg-toggle');
				if (!el) throw new Error('project toggle not yet rendered');
				return el;
			},
			{ timeout: 4000 }
		);

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'false'));
		const spy = (globalThis as Record<string, unknown>).__toolsCollapseToggleSpy as ReturnType<
			typeof fn
		>;
		await expect(spy).toHaveBeenCalledWith('sidebar:project:/Users/dev/temet-proxy');

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));
		const rowDisclose = canvasElement.querySelector<HTMLButtonElement>('.tpg-row__disclose');
		await expect(rowDisclose).toBeTruthy();
		await userEvent.click(rowDisclose!);
		await waitFor(() => expect(rowDisclose).toHaveAttribute('aria-expanded', 'true'));
		await expect(spy).toHaveBeenCalledWith('tu:1');
	}}
>
	{#snippet template()}
		{@const toggleSpy = fn()}
		{@const _registerSpy = (() => {
			(globalThis as Record<string, unknown>).__toolsCollapseToggleSpy = toggleSpy;
		})()}
		{@const collapseStoreImpl = (() => {
			const base = createCollapseStore();
			const store: CollapseStore = {
				isCollapsed: (key: string) => base.isCollapsed(key),
				isExpanded: (key: string, defaultExpanded?: boolean) =>
					base.isExpanded(key, defaultExpanded),
				toggle: (key: string) => {
					toggleSpy(key);
					base.toggle(key);
				},
				setDefault: (key: string, collapsed: boolean) => base.setDefault(key, collapsed),
				setAll: (keys: string[], collapsed: boolean) => base.setAll(keys, collapsed)
			};
			return store;
		})()}
		{@const _setCtx = setContext(COLLAPSE_STORE_KEY, collapseStoreImpl)}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="FilterApply"
	tags={['interactive']}
	loaders={[
		async () => {
			stubToolUsesFetch(buildFeedResponse(MIXED_ROWS));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				if (canvasElement.querySelectorAll('.tpg-project').length === 0) {
					throw new Error('groups not yet rendered');
				}
			},
			{ timeout: 4000 }
		);

		const onFetch = fn(async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/tool-uses/facets')) {
				return new Response(JSON.stringify(DEFAULT_FACETS), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/tool-uses')) {
				return new Response(JSON.stringify(buildFeedResponse(MIXED_ROWS)), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		});
		globalThis.fetch = onFetch as unknown as typeof fetch;

		const selects = canvasElement.querySelectorAll<HTMLSelectElement>('.tu-filter__select');
		const [projectSelect, toolSelect, timeSelect] = Array.from(selects);
		await waitFor(() => {
			if (!canvasElement.querySelector('option[value="/Users/dev/other-project"]')) {
				throw new Error('project option not yet rendered — facets fetch still pending');
			}
		});
		await userEvent.selectOptions(projectSelect!, '/Users/dev/other-project');
		await userEvent.selectOptions(toolSelect!, 'Edit');
		await userEvent.selectOptions(timeSelect!, '24h');

		await waitFor(() => {
			const dirty = canvasElement.querySelector('.filter-scope-status__dirty');
			if (!dirty) throw new Error('dirty indicator not yet rendered');
		});
		const dirtyNotice = canvasElement.querySelector('.filter-scope-status__dirty');
		await expect(dirtyNotice).toHaveTextContent('Selection changed');

		const checkbox = canvasElement.querySelector<HTMLInputElement>('.tu-checkbox');
		await expect(checkbox).toBeTruthy();
		await userEvent.click(checkbox!);

		const applyBtn = within(canvasElement).getByRole('button', { name: 'Apply' });
		await expect(applyBtn).toBeTruthy();
		await userEvent.click(applyBtn);

		await waitFor(
			() => {
				const hit = onFetch.mock.calls.some((args) => {
					const url = typeof args[0] === 'string' ? args[0] : String(args[0]);
					return (
						url.includes('/api/tool-uses') &&
						!url.includes('/facets') &&
						url.includes('project=') &&
						url.includes('tool_name=Edit') &&
						url.includes('since=') &&
						url.includes('errors_only=true')
					);
				});
				if (!hit) throw new Error('fetch with all four filter params not yet called');
			},
			{ timeout: 3000 }
		);
		await expect(onFetch).toHaveBeenCalled();

		await waitFor(() => {
			if (canvasElement.querySelector('.filter-scope-status__dirty')) {
				throw new Error('dirty indicator still shown after filters were submitted');
			}
		});
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="FilterClear"
	tags={['interactive']}
	loaders={[
		async () => {
			const bashRows: ToolUsesFeedRow[] = [
				buildRow({
					use_id: 400,
					tool_name: 'Bash',
					input: { command: 'make test' },
					is_error: true,
					result_id: 900
				})
			];
			stubToolUsesFetch(buildFeedResponse(bashRows));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools?tool_name=Bash#errors'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		await waitFor(
			() => {
				if (canvasElement.querySelectorAll('.tpg-project').length === 0) {
					throw new Error('groups not yet rendered');
				}
			},
			{ timeout: 4000 }
		);
		const checkbox = canvasElement.querySelector<HTMLInputElement>('.tu-checkbox');
		await expect(checkbox?.checked).toBe(true);

		const onClearFetch = fn(async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/tool-uses/facets')) {
				return new Response(JSON.stringify(DEFAULT_FACETS), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/tool-uses')) {
				return new Response(JSON.stringify(buildFeedResponse(MIXED_ROWS)), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		});
		globalThis.fetch = onClearFetch as unknown as typeof fetch;

		const clearBtn = within(canvasElement).getByRole('button', { name: 'Clear' });
		await expect(clearBtn).toBeTruthy();
		await userEvent.click(clearBtn);

		await waitFor(
			() => {
				const hit = onClearFetch.mock.calls.some((args) => {
					const url = typeof args[0] === 'string' ? args[0] : String(args[0]);
					return (
						url.includes('/api/tool-uses') &&
						!url.includes('/api/tool-uses/facets') &&
						!url.includes('tool_name=') &&
						!url.includes('errors_only=')
					);
				});
				if (!hit) throw new Error('unfiltered fetch not yet called');
			},
			{ timeout: 3000 }
		);
		await expect(onClearFetch).toHaveBeenCalled();
		await expect(checkbox?.checked).toBe(false);

		await new Promise((r) => setTimeout(r, 150));
		const toolUsesCallsAfterClear = onClearFetch.mock.calls.filter((args) => {
			const url = typeof args[0] === 'string' ? args[0] : String(args[0]);
			return url.includes('/api/tool-uses') && !url.includes('/facets');
		});
		await expect(toolUsesCallsAfterClear.length).toBe(1);
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="LoadMoreError"
	tags={['interactive']}
	loaders={[
		async () => {
			stubToolUsesFetch(buildFeedResponse(MIXED_ROWS.slice(0, 3), 200));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		__setRetryDelaysForTesting([0, 0, 0]);
		const loadMoreBtn = await waitFor(
			() => {
				const btn = canvasElement.querySelector<HTMLButtonElement>('.tu-footer__load-more');
				if (!btn) throw new Error('load-more button not yet rendered');
				return btn;
			},
			{ timeout: 4000 }
		);

		const onLoadMoreFetch = fn(async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/tool-uses/facets')) {
				return new Response(JSON.stringify(DEFAULT_FACETS), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/tool-uses')) {
				return new Response(JSON.stringify({ error: 'pagination unavailable' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		});
		globalThis.fetch = onLoadMoreFetch as unknown as typeof fetch;

		await userEvent.click(loadMoreBtn);
		await expect(onLoadMoreFetch).toHaveBeenCalled();

		const errSpan = await waitFor(
			() => {
				const el = canvasElement.querySelector('.tu-footer__error');
				if (!el) throw new Error('load-more error not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(errSpan).toHaveTextContent(/failed: 503/i);
		__resetRetryDelaysForTesting();
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="LoadingMoreState"
	tags={['interactive', 'data-driven']}
	loaders={[
		async () => {
			stubToolUsesFetch(buildFeedResponse(MIXED_ROWS.slice(0, 3), 200));
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/tools'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const loadMoreBtn = await waitFor(
			() => {
				const btn = canvasElement.querySelector<HTMLButtonElement>('.tu-footer__load-more');
				if (!btn) throw new Error('load-more button not yet rendered');
				return btn;
			},
			{ timeout: 4000 }
		);
		await expect(loadMoreBtn).not.toBeDisabled();

		let releaseLatch!: () => void;
		const latch = new Promise<void>((resolve) => {
			releaseLatch = resolve;
		});
		let callCount = 0;
		const onLoadMoreFetch = fn(async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/tool-uses/facets')) {
				return new Response(JSON.stringify(DEFAULT_FACETS), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/tool-uses')) {
				callCount++;
				if (callCount > 0) {
					await latch;
					const moreRows: ToolUsesFeedRow[] = Array.from({ length: 3 }, (_, i) =>
						buildRow({
							use_id: 900 + i,
							tool_name: 'Bash',
							input: { command: `echo "more row ${900 + i}"` }
						})
					);
					return new Response(JSON.stringify(buildFeedResponse(moreRows, 200)), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
			}
			return new Response('Not Found', { status: 404 });
		});
		globalThis.fetch = onLoadMoreFetch as unknown as typeof fetch;

		await userEvent.click(loadMoreBtn);

		await waitFor(
			() => {
				if (!loadMoreBtn.disabled) {
					throw new Error('expected load-more button to be disabled while in-flight');
				}
			},
			{ timeout: 2000 }
		);
		await expect(loadMoreBtn).toBeDisabled();
		await expect(loadMoreBtn).toHaveTextContent('Loading…');

		releaseLatch();

		await waitFor(
			() => {
				if (loadMoreBtn.disabled) {
					throw new Error('expected load-more button to re-enable after fetch');
				}
			},
			{ timeout: 3000 }
		);
		await expect(loadMoreBtn).not.toBeDisabled();
		await expect(onLoadMoreFetch).toHaveBeenCalled();
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[1200px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
