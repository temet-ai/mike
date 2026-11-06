<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import Page from './+page.svelte';
	import type { MemoryBreakdown, MemoryEntry } from '$lib/api/memory';

	function buildEntry(overrides: {
		id: number;
		session_id: string;
		status?: string;
		created_at?: string;
		request_body_bytes: number;
		response_body_bytes: number;
		filtered_request_body_bytes: number;
	}): MemoryEntry {
		return {
			id: overrides.id,
			session_id: overrides.session_id,
			status: overrides.status ?? 'Complete',
			created_at: overrides.created_at ?? '2026-05-17T12:00:00Z',
			request_body_bytes: overrides.request_body_bytes,
			response_body_bytes: overrides.response_body_bytes,
			filtered_request_body_bytes: overrides.filtered_request_body_bytes,
			total_bytes:
				overrides.request_body_bytes +
				overrides.response_body_bytes +
				overrides.filtered_request_body_bytes
		};
	}

	function buildSnapshot(overrides: Partial<MemoryBreakdown> = {}): MemoryBreakdown {
		return {
			entry_count: 0,
			request_body_bytes: 0,
			response_body_bytes: 0,
			filtered_request_body_bytes: 0,
			tool_calls_bytes: 0,
			tool_results_bytes: 0,
			tool_results_in_body_bytes: 0,
			system_prompt_blocks_bytes: 0,
			entries_total_bytes: 0,
			session_index_keys: 0,
			composable_session_index_keys: 0,
			session_meta_keys: 0,
			in_flight_count: 0,
			process_rss_bytes: null,
			max_entries: 5000,
			max_store_bytes: 268_435_456,
			over_byte_budget: false,
			top_entries: [],
			...overrides
		};
	}

	const SESSION_FE = '01J9X4QF7H8K2N3P5R6T7V8W9Y-frontend-route-blitz';
	const SESSION_RS = '01J9X4QG8J9L3M4N6S7U8W9X0Z-rust-developer-handlers';
	const SESSION_DB = '01J9X4QH9K0M4N5P7T8V9X0Y1A-db-dev-migrations';
	const SESSION_UI = '01J9X4QJ0L1N5P6R8U9W0Y1Z2B-ui-designer-styleguide';

	const TOP_ENTRIES: MemoryEntry[] = [
		buildEntry({
			id: 4821,
			session_id: SESSION_FE,
			status: 'Complete',
			created_at: '2026-05-17T11:58:42Z',
			request_body_bytes: 18_446_215,
			response_body_bytes: 41_002_887,
			filtered_request_body_bytes: 18_201_004
		}),
		buildEntry({
			id: 4790,
			session_id: SESSION_RS,
			status: 'Complete',
			created_at: '2026-05-17T11:55:10Z',
			request_body_bytes: 9_220_118,
			response_body_bytes: 22_887_440,
			filtered_request_body_bytes: 9_011_002
		}),
		buildEntry({
			id: 4755,
			session_id: SESSION_DB,
			status: 'Complete',
			created_at: '2026-05-17T11:50:33Z',
			request_body_bytes: 3_440_009,
			response_body_bytes: 8_120_551,
			filtered_request_body_bytes: 3_390_771
		}),
		buildEntry({
			id: 4712,
			session_id: SESSION_UI,
			status: 'Complete',
			created_at: '2026-05-17T11:44:01Z',
			request_body_bytes: 1_204_882,
			response_body_bytes: 2_998_310,
			filtered_request_body_bytes: 1_180_440
		})
	];

	const TWO_ENTRIES: MemoryEntry[] = [
		buildEntry({
			id: 4821,
			session_id: SESSION_FE,
			status: 'Complete',
			created_at: '2026-05-17T11:58:42Z',
			request_body_bytes: 18_446_215,
			response_body_bytes: 41_002_887,
			filtered_request_body_bytes: 18_201_004
		}),
		buildEntry({
			id: 4790,
			session_id: SESSION_RS,
			status: 'Complete',
			created_at: '2026-05-17T11:55:10Z',
			request_body_bytes: 9_220_118,
			response_body_bytes: 22_887_440,
			filtered_request_body_bytes: 9_011_002
		})
	];

	const POPULATED: MemoryBreakdown = buildSnapshot({
		entry_count: 412,
		request_body_bytes: 92_118_440,
		response_body_bytes: 188_002_551,
		filtered_request_body_bytes: 90_771_009,
		tool_calls_bytes: 14_882_310,
		tool_results_bytes: 22_004_118,
		tool_results_in_body_bytes: 8_119_004,
		system_prompt_blocks_bytes: 31_440_882,
		entries_total_bytes: 178_002_118,
		session_index_keys: 87,
		composable_session_index_keys: 64,
		session_meta_keys: 87,
		in_flight_count: 3,
		process_rss_bytes: 642_118_400,
		max_entries: 5000,
		max_store_bytes: 268_435_456,
		over_byte_budget: false,
		top_entries: TOP_ENTRIES
	});

	function stubMemoryFetch(snapshot: MemoryBreakdown): void {
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/diagnostics/memory')) {
				return new Response(JSON.stringify(snapshot), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	function stubMemoryFetchError(): void {
		globalThis.fetch = async () =>
			new Response(JSON.stringify({ error: 'Service Unavailable' }), {
				status: 503,
				statusText: 'Service Unavailable'
			});
	}

	const { Story } = defineMeta({
		title: 'Routes/Diagnostics/Memory',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Loading"
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
					url: new URL('http://localhost/diagnostics/memory'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1280px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	loaders={[
		async () => {
			stubMemoryFetchError();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/diagnostics/memory'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1280px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Populated"
	loaders={[
		async () => {
			stubMemoryFetch(POPULATED);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/diagnostics/memory'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1280px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="BudgetDisabled"
	loaders={[
		async () => {
			stubMemoryFetch(
				buildSnapshot({
					entry_count: 58,
					request_body_bytes: 4_118_002,
					response_body_bytes: 9_220_551,
					filtered_request_body_bytes: 4_001_119,
					entries_total_bytes: 17_339_672,
					session_index_keys: 12,
					composable_session_index_keys: 9,
					session_meta_keys: 12,
					in_flight_count: 0,
					process_rss_bytes: null,
					max_store_bytes: 0,
					over_byte_budget: false,
					top_entries: TWO_ENTRIES
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/diagnostics/memory'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1280px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="OverBudget"
	loaders={[
		async () => {
			stubMemoryFetch(
				buildSnapshot({
					entry_count: 4998,
					request_body_bytes: 142_118_440,
					response_body_bytes: 268_002_551,
					filtered_request_body_bytes: 140_771_009,
					tool_calls_bytes: 28_882_310,
					tool_results_bytes: 41_004_118,
					tool_results_in_body_bytes: 18_119_004,
					system_prompt_blocks_bytes: 61_440_882,
					entries_total_bytes: 301_118_440,
					session_index_keys: 211,
					composable_session_index_keys: 188,
					session_meta_keys: 211,
					in_flight_count: 12,
					process_rss_bytes: 1_204_882_944,
					max_store_bytes: 268_435_456,
					over_byte_budget: true,
					top_entries: TOP_ENTRIES
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/diagnostics/memory'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1280px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="EmptyStore"
	loaders={[
		async () => {
			stubMemoryFetch(
				buildSnapshot({
					entry_count: 0,
					process_rss_bytes: 188_440_576,
					top_entries: []
				})
			);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/diagnostics/memory'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1280px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
