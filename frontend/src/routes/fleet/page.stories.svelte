<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import { shortSessionId } from '$lib/format';
	import { proxyEvents } from '$lib/ws.svelte';
	import { acceptanceStatusFixture } from '$lib/fleet/__fixtures__/acceptance';
	import type {
		FleetGraphEdge,
		FleetGraphNode,
		FleetGraphResponse,
		FleetProjectSummary
	} from '$lib/fleet/types';

	const PROJECT = 'projects/temet/temet-proxy';

	const PROJECTS: FleetProjectSummary[] = [
		{
			project_path: PROJECT,
			session_count: 130,
			request_count: 106936,
			first_seen: '2026-06-09T06:44:47Z',
			last_seen: '2026-07-14T06:11:00Z',
			pinned: false,
			custom_name: null
		}
	];

	const NOW_MS = Date.now();
	const iso = (offsetMs: number): string => new Date(NOW_MS + offsetMs).toISOString();
	const MIN = 60_000;
	const HOUR = 3_600_000;

	const COORD_ID = 'df05f016-6cc9-4a32-8639-9c0dfb75d0a5';
	const SECOND_HUB_ID = '5668ba2c-0000-4000-8000-000000000001';
	const DEBUG_ID = '5282cc68-d231-46b7-8f7f-6e304152fbc4';
	const CFC0_ID = 'cfc0d8f3-9d96-4b1c-8af5-5644463e1efc';
	const REBASE_ID = '5841f1da-3f30-43bc-ac10-977242f39a1b';
	const AGENT_REG_ID = '328b621c-9db4-420c-99b4-e8ce93207d07';
	const FRONTEND_DEV_ID = '045ba74e-0000-4000-8000-000000000002';
	const RUST_DEV_ID = '5d5b8d89-0000-4000-8000-000000000003';
	const ALIGNMENT_ID = '54311714-0000-4000-8000-000000000004';
	const RUST_DEV_2_ID = '9770e3f6-0000-4000-8000-000000000005';
	const OBSERVER_ID = '872d7da7-a414-4432-bef2-630c65ef66b5';
	const REVIEWER_ID = '0e66116e-1511-4d7d-aa1c-cd1ab3058155';
	const BRIDGE_LEAF_ID = '3cdcb4f7-0000-4000-8000-000000000006';

	const DB_DEV_ID = 'aaaa0001-0000-4000-8000-000000000010';
	const UI_DESIGNER_ID = 'aaaa0002-0000-4000-8000-000000000011';
	const WATCHDOG_ID = 'aaaa0003-0000-4000-8000-000000000012';
	const TECH_WRITER_ID = 'aaaa0004-0000-4000-8000-000000000013';
	const RESEARCHER_ID = 'aaaa0005-0000-4000-8000-000000000014';

	function node(partial: Partial<FleetGraphNode> & { session_id: string }): FleetGraphNode {
		return {
			title: null,
			agent_type: null,
			role: null,
			model: 'claude-haiku-4-5-20251001',
			workspace: null,
			cwd: '/Users/vimes/projects/temet/temet-proxy',
			worktree: null,
			goal: null,
			next_steps: null,
			state: 'done',
			last_activity_at: iso(-2 * HOUR),
			request_count: 20,
			...partial
		};
	}

	function messaged(from: string, to: string, count: number): FleetGraphEdge {
		return {
			from_session_id: from,
			to_session_id: to,
			kind: 'messaged',
			first_ts: iso(-8 * HOUR),
			last_ts: iso(-5 * MIN),
			count
		};
	}

	const coreNodes: FleetGraphNode[] = [
		node({
			session_id: COORD_ID,
			title: 'Locate fleet page work',
			role: 'coordinator',
			model: 'claude-opus-4-6-20250915',
			state: 'working',
			state_detail: null,
			goal: 'Ship a fleet board that actually looks like this fleet.',
			next_steps: 'Drive :9991/fleet in a browser and judge the rebuilt board.',
			cwd: '/Users/vimes/projects/temet/temet-proxy/.claude/worktrees/agent-afb0898630c1b0e28',
			worktree: 'agent-afb0898630c1b0e28',
			unconsumed_mail: 2,
			last_activity_at: iso(-10_000),
			request_count: 412
		}),
		node({
			session_id: SECOND_HUB_ID,
			title: 'Commit and push turn-card expand fix',
			state: 'error',
			error_kind: 'agent',
			state_detail: 'Error: rate limited',
			last_activity_at: iso(-3 * MIN),
			request_count: 31
		}),
		node({
			session_id: DEBUG_ID,
			title: 'Debug steering session df05f016',
			last_activity_at: iso(-6 * HOUR),
			request_count: 18
		}),
		node({
			session_id: CFC0_ID,
			title: 'Rebuild fleet /api/fleet/graph truth model',
			state: 'error',
			error_kind: 'infra',
			state_detail: 'Error: proxy_crashed_during_stream',
			last_activity_at: iso(-40 * MIN),
			request_count: 9
		}),
		node({
			session_id: REBASE_ID,
			title: 'Rebase local main from remote',
			state: 'done',
			quality: {
				outcome: 'partial',
				rationale: 'Rebase completed but two conflicts required manual resolution.'
			},
			last_activity_at: iso(-4 * HOUR),
			request_count: 14
		}),
		node({
			session_id: AGENT_REG_ID,
			title: 'Register agent identity token slots',
			unconsumed_mail: 1,
			last_activity_at: iso(-90 * MIN),
			request_count: 11
		}),
		node({
			session_id: FRONTEND_DEV_ID,
			title: 'Rebuild fleet board frontend from design',
			agent_type: 'frontend-developer',
			role: 'frontend-developer',
			state: 'idle',
			last_activity_at: iso(-2 * MIN),
			request_count: 96
		}),
		node({
			session_id: RUST_DEV_ID,
			title: 'Fleet truth-model backend rebuild',
			agent_type: 'rust-developer',
			role: 'rust-developer',
			state: 'stale',
			last_activity_at: iso(-9 * HOUR),
			request_count: 88
		}),
		node({
			session_id: ALIGNMENT_ID,
			role: 'alignment',
			last_activity_at: iso(-5 * HOUR),
			request_count: 22
		}),
		node({
			session_id: RUST_DEV_2_ID,
			title: 'Fix self-loop edges in fleet graph',
			agent_type: 'rust-developer',
			role: 'rust-developer',
			last_activity_at: iso(-3 * HOUR),
			request_count: 34
		}),
		node({
			session_id: OBSERVER_ID,
			role: 'observer',
			last_activity_at: iso(-4 * HOUR),
			request_count: 27
		}),
		node({
			session_id: REVIEWER_ID,
			role: 'reviewer',
			state: 'idle',
			last_activity_at: iso(-30 * MIN),
			request_count: 6
		}),
		node({ session_id: BRIDGE_LEAF_ID, last_activity_at: iso(-7 * HOUR), request_count: 4 }),
		node({
			session_id: DB_DEV_ID,
			role: 'db-dev',
			last_activity_at: iso(-6 * HOUR),
			request_count: 41
		}),
		node({
			session_id: UI_DESIGNER_ID,
			role: 'ui-designer',
			last_activity_at: iso(-8 * HOUR),
			request_count: 19
		}),
		node({
			session_id: WATCHDOG_ID,
			role: 'watchdog',
			last_activity_at: iso(-10 * HOUR),
			request_count: 7
		}),
		node({
			session_id: TECH_WRITER_ID,
			role: 'technical-writer',
			last_activity_at: iso(-11 * HOUR),
			request_count: 13
		}),
		node({
			session_id: RESEARCHER_ID,
			role: 'researcher',
			last_activity_at: iso(-12 * HOUR),
			request_count: 5
		})
	];

	function fillerId(i: number): string {
		return `f0000000-0000-4000-8000-${String(i).padStart(12, '0')}`;
	}
	const fillerNodes: FleetGraphNode[] = [];
	for (let i = 0; i < 100; i++) {
		fillerNodes.push(
			node({
				session_id: fillerId(i),
				state: 'done',
				last_activity_at: iso(-(1 * HOUR + i * 8 * MIN)),
				request_count: (i % 12) + 1
			})
		);
	}
	for (let i = 0; i < 6; i++) {
		fillerNodes.push(
			node({
				session_id: fillerId(100 + i),
				state: 'error',
				error_kind: i % 2 === 0 ? 'ended_early' : 'agent',
				state_detail: i % 2 === 0 ? 'Error: stream aborted' : `Error: tool call failed (${i})`,
				last_activity_at: iso(-(3 * HOUR + i * HOUR)),
				request_count: i + 2
			})
		);
	}
	for (let i = 0; i < 4; i++) {
		fillerNodes.push(
			node({
				session_id: fillerId(106 + i),
				state: 'unknown',
				last_activity_at: iso(-(6 * HOUR + i * HOUR)),
				request_count: 1
			})
		);
	}
	for (let i = 0; i < 2; i++) {
		fillerNodes.push(
			node({
				session_id: fillerId(110 + i),
				state: 'done',
				last_activity_at: iso(-(2 * MIN + i * MIN)),
				request_count: 3
			})
		);
	}

	const ALL_NODES = [...coreNodes, ...fillerNodes];
	const ROLED_COUNT = ALL_NODES.filter((n) => n.role !== null).length;

	const GRAPH: FleetGraphResponse = {
		project_path: PROJECT,
		generated_at: iso(0),
		nodes: ALL_NODES,
		edges: [
			messaged(COORD_ID, RUST_DEV_ID, 9),
			messaged(RUST_DEV_ID, COORD_ID, 6),
			messaged(COORD_ID, FRONTEND_DEV_ID, 6),
			messaged(FRONTEND_DEV_ID, COORD_ID, 3),
			messaged(COORD_ID, REVIEWER_ID, 3),
			messaged(REVIEWER_ID, COORD_ID, 4),
			messaged(COORD_ID, RUST_DEV_2_ID, 3),
			messaged(RUST_DEV_2_ID, COORD_ID, 1),
			messaged(COORD_ID, ALIGNMENT_ID, 2),
			messaged(ALIGNMENT_ID, COORD_ID, 3),
			messaged(COORD_ID, OBSERVER_ID, 2),
			messaged(OBSERVER_ID, COORD_ID, 5),
			messaged(COORD_ID, REBASE_ID, 2),
			messaged(REBASE_ID, COORD_ID, 4),
			messaged(RUST_DEV_ID, FRONTEND_DEV_ID, 3),
			messaged(FRONTEND_DEV_ID, RUST_DEV_ID, 2),
			messaged(SECOND_HUB_ID, REBASE_ID, 1),
			messaged(SECOND_HUB_ID, DEBUG_ID, 1),
			messaged(SECOND_HUB_ID, CFC0_ID, 1),
			messaged(SECOND_HUB_ID, AGENT_REG_ID, 1),
			messaged(SECOND_HUB_ID, BRIDGE_LEAF_ID, 1),
			messaged(DEBUG_ID, AGENT_REG_ID, 1),
			messaged(DEBUG_ID, CFC0_ID, 1)
		]
	};

	const NODES_BY_SESSION_ID = new Map(ALL_NODES.map((n) => [n.session_id, n]));

	function installFleetFetchStub(): void {
		globalThis.fetch = async (input: Request | string | URL) => {
			const url =
				typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
			const json = (body: unknown, status = 200) =>
				new Response(JSON.stringify(body), {
					status,
					headers: { 'content-type': 'application/json' }
				});
			if (url.includes('/api/fleet/graph')) return json(GRAPH);
			if (url.includes('/api/fleets/by-session/')) {
				return url.includes(COORD_ID) ? json(acceptanceStatusFixture()) : json({}, 404);
			}
			if (url.includes('/api/projects')) return json(PROJECTS);
			const goalMatch = /\/api\/sessions\/([^/]+)\/goal/.exec(url);
			if (goalMatch) {
				const node = NODES_BY_SESSION_ID.get(decodeURIComponent(goalMatch[1] ?? ''));
				return json({
					goal: node?.goal ?? null,
					next_steps: node?.next_steps ?? null,
					role: node?.role ?? null,
					name: null,
					instructions: null
				});
			}
			if (url.includes('/notes')) return json([]);
			return json({}, 404);
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Fleet/Board',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/fleet?view=board'),
					params: {}
				}
			}
		}
	}}
	loaders={[
		async () => {
			proxyEvents.clear();
			installFleetFetchStub();
			return {};
		}
	]}
	play={async ({ canvas, canvasElement }) => {
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-slot="fleet-board"]')).not.toBeNull()
		);
		await expect(await canvas.findByText('Needs attention')).toBeInTheDocument();
		await expect(canvasElement.querySelector('[data-slot="fleet-canvas-wrap"]')).toBeNull();

		const emptyPanelText =
			canvasElement.querySelector('[data-slot="fleet-inspector-empty"]')?.textContent ?? '';
		await expect(emptyPanelText).toContain(
			'Click any session to see what it is working on, its activity, and how it is linked.'
		);
		await expect(emptyPanelText).not.toMatch(/in the graph/i);

		await expect(await canvas.findByText('Locate fleet page work')).toBeInTheDocument();
		await expect(await canvas.findByText('Fleet truth-model backend rebuild')).toBeInTheDocument();
		await expect(
			await canvas.findByText('Rebuild fleet board frontend from design')
		).toBeInTheDocument();
		await expect(await canvas.findByText(shortSessionId(DEBUG_ID))).toBeInTheDocument();
		await expect((await canvas.findAllByText('role unknown')).length).toBeGreaterThan(0);

		await expect(ROLED_COUNT).toBe(12);
		await expect(ALL_NODES.length).toBe(130);

		const groupTitles = [...canvasElement.querySelectorAll('[data-slot="fleet-board-group"]')].map(
			(el) => el.getAttribute('data-group-kind')
		);
		await expect(groupTitles[0]).toBe('attention');
		await expect(groupTitles).toContain('active-now');
		await expect(groupTitles).toContain('active-fleet');
		for (const role of [
			'coordinator',
			'frontend-developer',
			'rust-developer',
			'db-dev',
			'ui-designer'
		]) {
			await expect((await canvas.findAllByText(role)).length).toBeGreaterThanOrEqual(1);
		}
		const recentFillerRow = canvasElement.querySelector(
			`[data-node-id="${fillerId(110)}"]`
		) as HTMLElement;
		await expect(recentFillerRow).not.toBeNull();
		const activeNowGroup = canvasElement.querySelector(
			'[data-group-kind="active-now"]'
		) as HTMLElement;
		await expect(activeNowGroup.contains(recentFillerRow)).toBe(true);

		await expect(await canvas.findByText('Error: rate limited')).toBeInTheDocument();
		await expect(await canvas.findByText(/infra event/)).toBeInTheDocument();
		await expect(await canvas.findByText(/Error: proxy_crashed_during_stream/)).toBeInTheDocument();
		await expect((await canvas.findAllByText(/ended early/)).length).toBeGreaterThan(0);

		const rebaseRow = canvasElement.querySelector(`[data-node-id="${REBASE_ID}"]`) as HTMLElement;
		await expect(rebaseRow.getAttribute('data-node-state')).toBe('done');
		await expect(await canvas.findByText(/judge · partial/)).toBeInTheDocument();

		const rebaseIdCell = rebaseRow.querySelector('.flb-row__id') as HTMLElement;
		await userEvent.click(rebaseIdCell);
		await waitFor(() => expect(rebaseRow.className).toContain('flb-row--selected'));

		const searchInput = await canvas.findByRole('textbox', { name: 'Find session' });
		await userEvent.type(searchInput, 'rust-developer');
		const rustDevRow = () =>
			canvasElement.querySelector(`[data-node-id="${RUST_DEV_ID}"]`) as HTMLElement;
		const coordRow = () =>
			canvasElement.querySelector(`[data-node-id="${COORD_ID}"]`) as HTMLElement;
		await waitFor(() => expect(rustDevRow().className).not.toContain('flb-row--dimmed'));
		await expect(coordRow().className).toContain('flb-row--dimmed');
		await userEvent.clear(searchInput);
		await userEvent.type(searchInput, 'nonexistent-history-only-term-zzz');
		await expect(await canvas.findByText('no match in this view')).toBeInTheDocument();
		await userEvent.clear(searchInput);

		const relBtn = await canvas.findByRole('button', { name: /Relationships \(13\)/ });
		await relBtn.click();
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-slot="fleet-canvas-wrap"]')).not.toBeNull()
		);
		await expect(canvasElement.querySelector('[data-slot="fleet-board"]')).toBeNull();
		await expect(canvasElement.querySelectorAll('[data-slot="fleet-node"]').length).toBe(8);
		const boardBtn = await canvas.findByRole('button', { name: 'Board' });
		await boardBtn.click();
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-slot="fleet-board"]')).not.toBeNull()
		);

		const titleEl = canvasElement.querySelector('.flb-row__title') as HTMLElement;
		await expect(getComputedStyle(titleEl).overflowWrap).toBe('break-word');

		const coordRowMain = canvasElement
			.querySelector(`[data-node-id="${COORD_ID}"]`)
			?.closest('.flb-row-wrap')
			?.querySelector('.flb-row__main') as HTMLElement;
		await expect(coordRowMain.getBoundingClientRect().width).toBeGreaterThanOrEqual(160);

		const coordButton = canvasElement.querySelector(`[data-node-id="${COORD_ID}"]`) as HTMLElement;
		coordButton.click();
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-slot="fleet-acceptance-state"]')).toHaveTextContent(
				'Outstanding'
			)
		);
		await expect(
			await canvas.findByText('Ready for dependents', { exact: true })
		).toBeInTheDocument();
		await expect(await canvas.findByText('1 / 2 criteria accepted')).toBeInTheDocument();
		await expect(
			await canvas.findByText('Ship a fleet board that actually looks like this fleet.')
		).toBeInTheDocument();
		await expect(
			(await canvas.findAllByText(/agent-afb0898630c1b0e28/)).length
		).toBeGreaterThanOrEqual(2);
		const trafficLink = await canvas.findByRole('link', { name: 'open captured traffic →' });
		await expect(trafficLink).toHaveAttribute('href', `/sessions/${COORD_ID}`);

		const inspectorCommsLink = await canvas.findByRole('link', { name: 'open comms \u{2192}' });
		await expect(inspectorCommsLink).toHaveAttribute('href', `/sessions/${COORD_ID}/comms`);

		const coordCommsLink = coordButton
			.closest('.flb-row-wrap')
			?.querySelector('.flb-row__comms-link') as HTMLAnchorElement;
		await expect(coordCommsLink).toHaveAttribute('href', `/sessions/${COORD_ID}/comms`);
		await expect(coordCommsLink.className).toContain('flb-row__comms-link--active');

		const dbDevRow = canvasElement.querySelector(`[data-node-id="${DB_DEV_ID}"]`) as HTMLElement;
		const dbDevCommsLink = dbDevRow
			.closest('.flb-row-wrap')
			?.querySelector('.flb-row__comms-link') as HTMLAnchorElement;
		await expect(dbDevCommsLink).toHaveAttribute('href', `/sessions/${DB_DEV_ID}/comms`);
		await expect(dbDevCommsLink.className).not.toContain('flb-row__comms-link--active');
		await userEvent.click(dbDevRow);
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-slot="fleet-acceptance"]')).toBeNull()
		);
	}}
>
	{#snippet template()}
		<div class="h-[760px] w-[1200px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
