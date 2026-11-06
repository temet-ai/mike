<script lang="ts">
	import { getContext, onMount } from 'svelte';
	import { SESSION_LIST_STORE_KEY, STATS_KEY, type StatsContext } from '$lib/context-keys';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import { deriveSessionActivity } from '$lib/sessionActivity';
	import { formatCount } from '$lib/time';
	import { shellState } from '$lib/stores/shellState.svelte';
	import { GUIDES } from '$lib/help/guides';
	import PageTour, { type TourStep } from '$lib/components/shared/PageTour.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import PulseStrip from '$lib/components/home/PulseStrip.svelte';
	import NowBand from '$lib/components/home/NowBand.svelte';
	import CapabilityBanner from '$lib/components/home/CapabilityBanner.svelte';
	import DestinationGroup from '$lib/components/home/DestinationGroup.svelte';
	import DestinationCard from '$lib/components/home/DestinationCard.svelte';
	import { seamMarkersAction } from '$lib/actions/seamMarkers';
	import { Button } from '$lib/components/ui/button';
	import { Compass } from '@lucide/svelte';
	import { revealSections } from '$lib/actions/reveal';
	import HomeLaunchpad from '$lib/components/home/HomeLaunchpad.svelte';
	import ScheduledJobsPanel from '$lib/components/home/ScheduledJobsPanel.svelte';
	import { getSkillProjects } from '$lib/api/skills';

	const sessionStore = getContext<SessionListStore>(SESSION_LIST_STORE_KEY);
	const statsCtx = getContext<StatsContext>(STATS_KEY);

	function openHelp(): void {
		shellState.helpDrawerOpen = true;
	}

	let skillsWaiting = $state('—');
	onMount(() => {
		getSkillProjects()
			.then((r) => (skillsWaiting = formatCount(r.waiting)))
			.catch(() => (skillsWaiting = '—'));
	});

	let now = $state(Date.now());
	onMount(() => {
		const id = setInterval(() => {
			now = Date.now();
		}, 1_000);
		return () => clearInterval(id);
	});

	const sessions = $derived(sessionStore?.sessions ?? []);
	const sessionsLoading = $derived(sessionStore?.loading ?? false);
	const hasMoreSessions = $derived(sessionStore?.hasMore ?? false);
	const stats = $derived(statsCtx?.value ?? null);

	const activeCount = $derived.by(() => {
		if (!sessionStore) return 0;
		let count = 0;
		for (const s of sessions) {
			const wsEvent = sessionStore.latestWsEventForSession(s.id);
			if (deriveSessionActivity(s, wsEvent, now).isActive) count += 1;
		}
		return count;
	});

	const isColdStart = $derived(
		stats !== null && !sessionsLoading && stats.request_count === 0 && sessions.length === 0
	);

	let tourOpen = $state(false);

	onMount(() => {
		if (new URLSearchParams(window.location.search).get('tour') === '1') tourOpen = true;
	});

	const HOME_TOUR_STEPS: TourStep[] = [
		{
			target: '[data-slot="pulse-strip"]',
			title: 'The live pulse',
			body: 'Uptime, requests captured, stored sessions and open streams, updated as traffic passes through. Click the strip to open the full session list.'
		},
		{
			target: '[data-slot="home-launchpad"]',
			title: 'Start from a task',
			body: 'Not sure where to go? Pick what you want to do (understand a session, shape traffic, or watch the fleet) and follow the trail.'
		},
		{
			target: '[data-slot="now-band"]',
			title: 'Sessions running now',
			body: 'Only the sessions active right now, grouped by project. Open a row to read its turns as they arrive.'
		},
		{
			target: '[data-slot="destination-group"][data-accent="insight"]',
			title: 'Insight: read what happened',
			body: 'Sessions, analysis, prompts, agents, tools and diagnostics. These pages only read captured traffic; nothing here changes a request.'
		},
		{
			target: '[data-slot="destination-group"][data-accent="levers"]',
			title: 'Levers: change the traffic',
			body: 'Filters rewrite requests on the wire and steering corrects sessions that drift. Dry-run a filter against captured traffic before you turn it on.'
		},
		{
			target: '[data-slot="destination-group"][data-accent="system"]',
			title: 'System, and where to learn more',
			body: 'Memory, settings and help. Every card has Open and, where there is one, How it works. Replay this tour any time from Take a tour.'
		}
	];
</script>

<svelte:head>
	<title>Home — mike</title>
</svelte:head>

{#if isColdStart}
	<section
		class="col detail flex min-h-0 flex-1 flex-col items-center gap-4 overflow-y-auto px-6 pt-4"
		aria-label="Home"
		data-slot="home-cold-start"
		use:revealSections
	>
		<h1 class="sr-only">Home</h1>
		<CapabilityBanner />
		<div class="flex min-h-0 flex-1 items-center">
			<EmptyStateOnboarding onOpenHelp={openHelp} />
		</div>
	</section>
{:else}
	<section
		class="home-page flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto px-6 py-5"
		aria-label="Home"
		data-slot="home-page"
		use:revealSections
	>
		<div data-reveal>
			<PageIntro
				title="Home"
				level="h1"
				description="Live sessions, captured traffic and the controls to shape what happens next."
			>
				{#snippet actions()}
					<Button variant="outline" onclick={() => (tourOpen = true)}>
						<Compass class="size-4" aria-hidden="true" />
						Take a tour
					</Button>
				{/snippet}
			</PageIntro>
		</div>

		<CapabilityBanner />

		<PulseStrip
			uptimeSeconds={stats?.uptime_seconds ?? null}
			requestCount={stats?.request_count ?? null}
			sessionsStored={sessions.length}
			{hasMoreSessions}
			activeStreams={stats?.active_streams ?? null}
			activeSessionsCount={activeCount}
		/>

		<ScheduledJobsPanel {now} />

		<HomeLaunchpad />

		<div class="home-groups" use:seamMarkersAction>
			{#if sessionStore}
				<NowBand {sessionStore} {now} />
			{/if}

			<DestinationGroup
				accent="insight"
				title="Insight"
				purpose="Understand the traffic — what was captured, explained."
			>
				<DestinationCard
					group="insight"
					tag="Sessions"
					whatItIs="Every session captured — the full log behind the live rows up top, searchable and replayable."
					why={[
						'Reopen a session from last week and read its turns',
						'Search across sessions for a prompt or model'
					]}
					stat={{
						value: `${formatCount(sessions.length)}${hasMoreSessions ? '+' : ''}`,
						label: 'sessions stored'
					}}
					openHref="/sessions"
					howItWorksHref="/help/guides/inspecting-requests"
				/>
				<DestinationCard
					group="insight"
					tag="Analysis"
					whatItIs="Automated findings over captured traffic — overselling, blame-deflection, faked understanding, by category."
					why={[
						'Audit yesterday’s sessions for weasel patterns',
						'Trace a finding to the exact request that produced it'
					]}
					openHref="/analysis"
				/>
				<DestinationCard
					group="insight"
					tag="Prompts"
					whatItIs="The system prompts actually sent — composition layers, anomalies, and how they change over time."
					why={[
						'See exactly what was injected into a turn',
						'Diff a prompt against the previous session’s'
					]}
					openHref="/prompts"
					howItWorksHref="/help/guides/anomalies"
				/>
				<DestinationCard
					group="insight"
					tag="Agents"
					whatItIs="Subagent dispatches — which agents ran, under whom, and what they returned."
					why={[
						'Follow a dispatch from parent turn to returned result',
						'Spot the agent that never reported back'
					]}
					openHref="/agents"
				/>
				<DestinationCard
					group="insight"
					tag="Tools"
					whatItIs="Tool-call traffic — usage stats, errors, and per-session drill-downs."
					why={[
						'Find the tool that fails most across sessions',
						'Replay a failing call’s exact input and result'
					]}
					openHref="/tools"
				/>
				<DestinationCard
					group="insight"
					tag="Diagnostics"
					whatItIs="Health findings and capture-store diagnostics."
					why={['Check capture integrity after an upgrade', 'Find what is bloating the store']}
					openHref="/diagnostics"
				/>
				<DestinationCard
					group="insight"
					tag="Projects"
					whatItIs="Every codebase or workspace this has captured traffic for."
					why={[
						'See how many sessions and requests came from a project',
						'Jump from a session or tool call to the project it came from'
					]}
					openHref="/projects"
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
					whatItIs="Rewrite rules applied to requests before they reach Anthropic — redact, replace, or re-inject content on the wire."
					why={[
						'Strip a leaked secret from every outgoing request',
						'Dry-run a rule against captured traffic before enabling it'
					]}
					openHref="/filter/workbench"
					howItWorksHref="/help/guides/filter-rules"
				/>
				<DestinationCard
					group="levers"
					tag="Steering"
					whatItIs="Judges that watch live sessions and inject a correction when a session claims done falsely or drifts off task."
					why={[
						'A session said it finished but didn’t → catch it in the act',
						'A session drifted off task → nudge it back onto its actual ask'
					]}
					openHref="/steering"
					howItWorksHref="/help/guides/steering-concepts"
				/>
				<DestinationCard
					group="levers"
					tag="Steerer monitor"
					whatItIs="The fleet view of every steerer — what fired, on which session, with what verdict and confidence."
					why={[
						'See which steerer fired overnight, and why',
						'Tune a judge that fires too often on honest turns'
					]}
					openHref="/steerers"
				/>
				<DestinationCard
					group="levers"
					tag="Fleet"
					whatItIs="Multi-session coordination — the workspace roster, peer messaging, and who is working on what."
					why={[
						'Find which peer session is holding a file you need',
						'Message a running session without touching its terminal'
					]}
					openHref="/fleet"
				/>
			</DestinationGroup>

			<DestinationGroup
				accent="system"
				title="System"
				purpose="Keep it running — memory, configuration, and help."
			>
				<DestinationCard
					group="system"
					tag="Memory"
					whatItIs="Lessons and recalls stored here and re-injected into future sessions."
					why={[
						'See which lessons fired in a session',
						'Record a correction so it sticks next time'
					]}
					openHref="/memory"
				/>
				<DestinationCard
					group="system"
					tag="Skills"
					whatItIs="Skills mike writes into your project folders from the lessons it learned. Read each one and decide whether it ships."
					why={[
						'Approve or reject a skill before it reaches a project',
						'See where a skill lives and whether sessions load it'
					]}
					stat={{ value: skillsWaiting, label: 'skills waiting on you' }}
					openHref="/skills"
				/>
				<DestinationCard
					group="system"
					tag="Settings"
					whatItIs="Runtime feature toggles — the live source of truth for configuration."
					why={['Flip a feature without a restart', 'Check what the boot config resolved to']}
					stat={{ value: 'All', label: 'toggles on' }}
					openHref="/settings"
				/>
				<DestinationCard
					group="system"
					tag="Help"
					whatItIs="Guides and the glossary for everything above — start here if a term is new."
					why={[
						'Read the getting-started guide end to end',
						'Look up a term you just met on another page'
					]}
					stat={{ value: String(GUIDES.length), label: 'guides · glossary' }}
					openHref="/help"
				/>
			</DestinationGroup>
		</div>
	</section>

	<PageTour bind:open={tourOpen} steps={HOME_TOUR_STEPS} label="Home tour" />
{/if}

<style>
	.home-groups {
		display: flex;
		flex-direction: column;
		gap: 0;
	}
	.home-groups > :global(.dg:not(:last-child)),
	.home-groups > :global(.now-band:not(:last-child)) {
		--aug-bl: 0px;
		--aug-br1: 0px;
		--aug-br2: 0px;
	}
	.home-groups > :global(.dg:not(:first-child)),
	.home-groups > :global(.now-band:not(:first-child)) {
		--aug-tl: 0px;
		--aug-tr1: 0px;
		--aug-tr2: 0px;
	}
</style>
