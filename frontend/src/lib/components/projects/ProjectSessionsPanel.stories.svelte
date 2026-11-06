<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import ProjectSessionsPanel from './ProjectSessionsPanel.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	import { buildSessionSummary } from '$lib/components/__fixtures__/session';
	import type { SessionSummary } from '$lib/types';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const PROJECT_PATH = '/Users/dev/myproject';
	const SIBLING_PROJECT_PATH = '/Users/dev/myproject-sibling';
	const SESSION_COUNT = 20;

	const PROJECT: FleetProjectWithCounts = {
		project_path: PROJECT_PATH,
		session_count: SESSION_COUNT,
		request_count: 4100,
		first_seen: '2025-11-01T00:00:00Z',
		last_seen: '2026-08-15T09:00:00Z',
		pinned: false,
		custom_name: null
	};

	const REF_MS = Date.parse('2026-08-15T12:00:00Z');
	function isoDaysAgo(days: number): string {
		return new Date(REF_MS - days * 24 * 60 * 60 * 1000).toISOString();
	}
	function uuidFor(n: number): string {
		return `a1b2c3d4-e5f6-4789-8abc-${String(n).padStart(12, '0')}`;
	}

	const SIBLING_ROW = buildSessionSummary({
		id: uuidFor(9999),
		project: SIBLING_PROJECT_PATH,
		title: 'Belongs to a sibling project — must be filtered out',
		first_seen: isoDaysAgo(1),
		last_seen: isoDaysAgo(1),
		request_count: 3
	});

	function exactRow(n: number): SessionSummary {
		return buildSessionSummary({
			id: uuidFor(n),
			project: PROJECT_PATH,
			title:
				n === 1 ? 'Implement retry backoff for the ingest worker' : `Session ${n} in this project`,
			first_seen: isoDaysAgo(90 - n),
			last_seen: isoDaysAgo(90 - n),
			request_count: 4 + (n % 5)
		});
	}

	const WEEK_RAW: SessionSummary[] = [
		SIBLING_ROW,
		...Array.from({ length: 5 }, (_, i) => exactRow(i + 1))
	];
	const ALLTIME_RAW: SessionSummary[] = [
		SIBLING_ROW,
		...Array.from({ length: SESSION_COUNT }, (_, i) => exactRow(i + 1))
	];
	const SEARCH_RAW: SessionSummary[] = [exactRow(1)];

	function router(method: string, url: string, body: unknown): TypedBody {
		if (method === 'PATCH') {
			const match = /\/api\/sessions\/([^/]+)\/pin/.exec(url);
			const id = match ? decodeURIComponent(match[1]!) : '';
			const pinned = (body as { pinned: boolean }).pinned;
			return typed<{ session_id: string; pinned: boolean }>({ session_id: id, pinned });
		}
		const u = new URL(url, 'http://localhost');
		const project = u.searchParams.get('project');
		if (project !== PROJECT_PATH) return typed<SessionSummary[]>([]);
		const search = u.searchParams.get('search');
		if (search) {
			return typed<SessionSummary[]>(
				SEARCH_RAW.filter((s) => (s.title ?? '').toLowerCase().includes(search.toLowerCase()))
			);
		}
		const dateFrom = u.searchParams.get('date_from');
		const offset = Number(u.searchParams.get('offset') ?? '0');
		const limit = Number(u.searchParams.get('limit') ?? '15');
		const source = dateFrom ? ALLTIME_RAW : WEEK_RAW;
		return typed<SessionSummary[]>(source.slice(offset, offset + limit));
	}

	const { Story } = defineMeta({
		title: 'Projects/ProjectSessionsPanel',
		component: ProjectSessionsPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	args={{ project: PROJECT }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const wrapper = canvasElement.querySelector('.psp-story-wrap') as HTMLElement;
		const rows = () => canvasElement.querySelectorAll('.ssn-row-wrap');

		const toggle = canvas.getByRole('button', { name: /sessions/i, expanded: true });
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');

		await waitFor(() =>
			expect(canvas.getByText('5 sessions in the last 7 days · 20 all time')).toBeInTheDocument()
		);
		await expect(rows().length).toBe(5);
		await expect(canvas.queryByText(/Belongs to a sibling project/i)).toBeNull();

		const thead = canvasElement.querySelector('.psp-thead') as HTMLElement;
		await expect(getComputedStyle(thead).display).toBe('none');
		const firstIdFull = rows()[0]!.querySelector('.ssn-id-text-full');
		await expect(firstIdFull?.textContent).toBe(uuidFor(1));
		await expect(getComputedStyle(rows()[0]!.querySelector('.ssn-id-text')!).display).toBe('none');
		await expect(rows()[0]!.querySelector('.ssn-clock-label')?.textContent).toBe('Started ');
		await expect(getComputedStyle(rows()[0]!.querySelector('.ssn-req-unit')!).display).not.toBe(
			'none'
		);

		wrapper.style.width = '900px';
		await waitFor(() => expect(getComputedStyle(thead).display).not.toBe('none'));
		await expect(rows().length).toBe(5);

		const panel = toggle.closest('section') as HTMLElement;
		await expect(panel.getAttribute('data-augmented-ui')).toBe('tl-clip br-clip border');
		await expect(getComputedStyle(panel).clipPath).not.toBe('none');
		await expect(panel.hasAttribute('data-accent')).toBe(false);
		await expect(toggle.textContent?.trim()).toContain('Sessions');
		const marker = toggle.querySelector('.dm') as HTMLElement;
		await expect(marker).not.toBeNull();
		await expect(marker.classList.contains('dm--open')).toBe(true);
		await expect(
			canvasElement.querySelector('[data-slot="project-sessions-count"]')
		).not.toBeNull();

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'false'));
		// The count stays in the header (`headerActions`, always visible) —
		// only the collapsible body unmounts, via the shared `disclose`
		// transition, so wait for that to actually leave.
		await waitFor(() =>
			expect(
				canvas.queryByLabelText("Search this project's sessions by title or full id")
			).not.toBeInTheDocument()
		);
		await expect(marker.classList.contains('dm--open')).toBe(false);
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));
		await expect(
			canvas.getByLabelText("Search this project's sessions by title or full id")
		).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: 'All time' }));
		await waitFor(() =>
			expect(canvas.getByText('Showing 14 of 20 sessions, newest first')).toBeInTheDocument()
		);
		await expect(rows().length).toBe(14);
		const table = canvasElement.querySelector('.psp-table') as HTMLElement;
		await expect(table.hasAttribute('data-augmented-ui')).toBe(false);
		const loadOlderBtn = canvas.getByRole('button', { name: /load older sessions/i });
		await expect(loadOlderBtn).toBeInTheDocument();

		await userEvent.click(loadOlderBtn);
		await waitFor(() => expect(canvas.getByText('All 20 sessions')).toBeInTheDocument());
		await expect(rows().length).toBe(20);
		await expect(canvas.queryByRole('button', { name: /load older sessions/i })).toBeNull();

		const searchInput = canvas.getByLabelText("Search this project's sessions by title or full id");
		await userEvent.type(searchInput, 'retry');
		await waitFor(
			() => expect(canvas.getByText('1 matching session · 20 all time')).toBeInTheDocument(),
			{ timeout: 2000 }
		);
		await expect(rows().length).toBe(1);
		await expect(canvasElement.querySelector('.ssn-headline-text')?.textContent).toBe(
			'Implement retry backoff for the ingest worker'
		);

		await userEvent.click(canvas.getByRole('button', { name: 'Clear search' }));
		await waitFor(() =>
			expect(canvas.getByText('Showing 14 of 20 sessions, newest first')).toBeInTheDocument()
		);

		const firstRow = rows()[0] as HTMLElement;
		const pinBtn = within(firstRow).getByRole('button', { name: /pin session/i });
		await userEvent.click(pinBtn);
		await waitFor(() => expect(pinBtn).toHaveAttribute('aria-pressed', 'true'));
	}}
>
	{#snippet template(args)}
		<div class="psp-story-wrap" style="width: 440px; background: var(--surface-bg); padding: 24px;">
			<CollapseStoreProvider>
				<ProjectSessionsPanel {...args} />
			</CollapseStoreProvider>
		</div>
	{/snippet}
</Story>
