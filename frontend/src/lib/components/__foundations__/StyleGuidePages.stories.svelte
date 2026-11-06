<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { seamMarkersAction } from '$lib/actions/seamMarkers';
	import { buildMockSessionListStore } from '$lib/test-support/storyMocks';
	import { buildSessionSummary, buildDetailMetrics } from '$lib/components/__fixtures__/session';
	import PulseStrip from '$lib/components/home/PulseStrip.svelte';
	import HomeLaunchpad from '$lib/components/home/HomeLaunchpad.svelte';
	import NowBand from '$lib/components/home/NowBand.svelte';
	import DestinationGroup from '$lib/components/home/DestinationGroup.svelte';
	import DestinationCard from '$lib/components/home/DestinationCard.svelte';
	import { Compass } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import SearchField from '$lib/components/shared/SearchField.svelte';
	import OverviewBand from '$lib/components/shared/OverviewBand.svelte';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import PinToggle from '$lib/components/shared/PinToggle.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import SessionTableHead from '$lib/components/sessions/SessionTableHead.svelte';
	import SessionRowHarness from '$lib/components/sessions/__harness__/SessionRowHarness.svelte';
	import ProjectSessionGroupHarness from '$lib/components/sessions/__harness__/ProjectSessionGroupHarness.svelte';
	import type { ProjectGroupRow } from '$lib/components/sessions/ProjectSessionGroup.svelte';

	const { Story } = defineMeta({
		title: 'Foundations/Style Guide Pages',
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'The patterns the Home page and the Sessions page are built from, drawn with the ' +
						'components those pages import. Each plate names the file.'
				}
			}
		}
	});

	const NOW = Date.parse('2026-06-21T20:00:00Z');
	const at = (minutesAgo: number): string => new Date(NOW - minutesAgo * 60_000).toISOString();

	const LIVE = [
		buildSessionSummary({
			id: '11111111-0000-0000-0000-000000000001',
			project: '/Users/dev/alpha',
			title: 'Implement auth middleware',
			last_status: 'Streaming',
			request_count: 12,
			first_seen: at(60),
			last_seen: at(0)
		}),
		buildSessionSummary({
			id: '22222222-0000-0000-0000-000000000001',
			project: '/Users/dev/beta',
			title: 'Refactor DB migrations',
			last_status: 'Pending',
			request_count: 12,
			first_seen: at(60),
			last_seen: at(1)
		})
	];

	const ROWS = [
		buildSessionSummary({
			id: '66666666-0000-0000-0000-000000000001',
			project: '/Users/dev/alpha',
			title: 'Implement auth middleware',
			last_status: 'Complete',
			request_count: 34,
			first_seen: at(600),
			last_seen: at(555),
			pinned: true
		}),
		buildSessionSummary({
			id: '66666666-0000-0000-0000-000000000002',
			project: '/Users/dev/alpha',
			title: 'Fix rate limit headers',
			last_status: 'Error: ERR 507',
			request_count: 7,
			first_seen: at(2000),
			last_seen: at(1980),
			pending_inbox_count: 3
		}),
		buildSessionSummary({
			id: '66666666-0000-0000-0000-000000000003',
			project: '/Users/dev/alpha',
			title: 'Streaming now',
			last_status: 'Streaming',
			request_count: 2,
			first_seen: at(10),
			last_seen: at(0)
		})
	];

	const GROUP: ProjectGroupRow = {
		key: 'alpha',
		project: '/Users/dev/alpha',
		isUngrouped: false,
		name: 'alpha',
		path: '/Users/dev/alpha',
		sessions: ROWS,
		colorClass: 'ssn-chip--1'
	};

	const DETAIL = new Map(ROWS.map((row) => [row.id, buildDetailMetrics()]));

	let query = $state('');
	let pinned = $state(false);
	let open = $state(true);
</script>

{#snippet plate(name: string, file: string)}
	<div class="sg-plate-head">
		<span class="machine sg-name">{name}</span>
		<code class="sg-file">{file}</code>
	</div>
{/snippet}

<Story name="Home page">
	{#snippet template()}
		<div class="sg" data-design="funk">
			<h2 class="machine sg-title">Home page</h2>
			{@render plate('Page title with an action', '$lib/components/shared/PageIntro.svelte')}
			<PageIntro
				title="Home"
				level="h1"
				description="Live sessions, captured traffic and the controls to shape what happens next."
			>
				{#snippet actions()}
					<Button variant="outline">
						<Compass class="size-4" aria-hidden="true" />
						Take a tour
					</Button>
				{/snippet}
			</PageIntro>
			{@render plate('Status strip', '$lib/components/home/PulseStrip.svelte')}
			<PulseStrip
				uptimeSeconds={4 * 86_400 + 2 * 3_600}
				requestCount={128_430}
				sessionsStored={812}
				hasMoreSessions
				activeStreams={2}
				activeSessionsCount={2}
			/>
			{@render plate('Task cards', '$lib/components/home/HomeLaunchpad.svelte')}
			<HomeLaunchpad />
			{@render plate('Live sessions band', '$lib/components/home/NowBand.svelte')}
			<div use:seamMarkersAction>
				<NowBand sessionStore={buildMockSessionListStore({ sessions: LIVE })} now={NOW} />
			</div>
			{@render plate('Destination group', '$lib/components/home/DestinationGroup.svelte')}
			{@render plate('Destination card', '$lib/components/home/DestinationCard.svelte')}
			<div use:seamMarkersAction>
				<DestinationGroup
					accent="insight"
					title="Insight"
					purpose="Understand the traffic — what was captured, explained."
				>
					<DestinationCard
						group="insight"
						tag="Sessions"
						whatItIs="Every session captured, searchable and replayable."
						why={['Reopen a session from last week', 'Search across sessions for a prompt']}
						stat={{ value: '812+', label: 'sessions stored' }}
						openHref="/sessions"
						howItWorksHref="/help/guides/inspecting-requests"
					/>
					<DestinationCard
						group="insight"
						tag="Tools"
						whatItIs="Tool-call traffic: usage, errors and per-session drill-downs."
						why={['Find the tool that fails most', 'Replay a failing call']}
						openHref="/tools"
					/>
				</DestinationGroup>
				<DestinationGroup
					accent="levers"
					title="Levers"
					purpose="Change the traffic — these act on requests as they pass through."
				>
					<DestinationCard
						group="levers"
						tag="Filters"
						whatItIs="Rewrite rules applied to requests before they reach Anthropic."
						why={['Strip a leaked secret', 'Dry-run a rule before enabling it']}
						openHref="/filter/workbench"
					/>
				</DestinationGroup>
				<DestinationGroup
					accent="system"
					title="System"
					purpose="Keep it running — memory, configuration, and help."
				>
					<DestinationCard
						group="system"
						tag="Settings"
						whatItIs="Runtime feature toggles."
						why={['Flip a feature without a restart', 'Check what the boot config resolved to']}
						stat={{ value: 'All', label: 'toggles on' }}
						openHref="/settings"
					/>
				</DestinationGroup>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Sessions page">
	{#snippet template()}
		<div class="sg" data-design="funk">
			<h2 class="machine sg-title">Sessions page</h2>
			{@render plate('Page title with a search field', '$lib/components/shared/PageIntro.svelte')}
			{@render plate('Search field', '$lib/components/shared/SearchField.svelte')}
			<PageIntro
				title="Sessions"
				level="h1"
				description="Pick a project, open a session, follow the conversation."
			>
				{#snippet actions()}
					<SearchField
						bind:value={query}
						label="Search sessions or content"
						placeholder="Search sessions or content"
					/>
				{/snippet}
			</PageIntro>
			{@render plate('Overview band of counts', '$lib/components/shared/OverviewBand.svelte')}
			<OverviewBand
				label="Loaded session overview"
				facts={[
					{ label: 'Sessions loaded', value: 21 },
					{ label: 'Project groups', value: 4 }
				]}
			>
				Search a title, session ID, or anything said in a conversation. Expand a project to explore
				its history.
			</OverviewBand>
			{@render plate('Project group', '$lib/components/sessions/ProjectSessionGroup.svelte')}
			<ProjectSessionGroupHarness group={GROUP} now={NOW} detailMetrics={DETAIL} />
			{@render plate('Table head', '$lib/components/sessions/SessionTableHead.svelte')}
			<div class="sg-surface">
				<SessionTableHead />
			</div>
			{@render plate('Session row', '$lib/components/sessions/SessionRow.svelte')}
			<div class="sg-surface">
				{#each ROWS as row (row.id)}
					<SessionRowHarness session={row} now={NOW} detail={DETAIL.get(row.id)} />
				{/each}
			</div>
			{@render plate('Copy button', '$lib/components/shared/CopyIconButton.svelte')}
			{@render plate('Pin toggle', '$lib/components/shared/PinToggle.svelte')}
			{@render plate('Disclosure marker', '$lib/components/shared/DisclosureMarker.svelte')}
			<div class="sg-row">
				<CopyIconButton getText={() => '66666666'} label="Copy session id" />
				<PinToggle {pinned} onToggle={() => (pinned = !pinned)} />
				<button type="button" class="sg-toggle" aria-expanded={open} onclick={() => (open = !open)}>
					<DisclosureMarker {open} />
					<span>{open ? 'open' : 'closed'}</span>
				</button>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.sg {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		max-width: 1080px;
		padding: var(--space-6);
		background: var(--surface-bg);
		color: var(--text-1);
		font-size: var(--text-md);
	}
	.sg-title {
		margin: var(--space-3) 0 0;
		font-size: var(--text-lg);
		color: var(--text-strong);
	}
	.sg-plate-head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-3);
		margin-top: var(--space-4);
		padding-top: var(--space-3);
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.sg-name {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.sg-file {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--interactive-active-fg);
	}
	.sg-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-4);
	}
	.sg-surface {
		background: var(--surface-card);
	}
	.sg-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		border: 0;
		background: transparent;
		color: var(--text-2);
		font: inherit;
		cursor: pointer;
	}
</style>
