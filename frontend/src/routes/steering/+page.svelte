<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import SteeringMonitor from '$lib/components/live/SteeringMonitor.svelte';
	import SteererFleet from '$lib/components/live/SteererFleet.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import { revealSections } from '$lib/actions/reveal';
	import { fade } from 'svelte/transition';
	import { createLiveSteeringStore } from '$lib/state/liveSteering.svelte';

	let now = $state(Date.now());

	const liveStore = createLiveSteeringStore();

	let fleetCount = $state<number | null>(null);

	onMount(() => {
		const iv = setInterval(() => {
			now = Date.now();
		}, 5_000);
		const stopLive = liveStore.start();
		return () => {
			clearInterval(iv);
			stopLive();
		};
	});

	const activeTab = $derived(
		(page.url.searchParams.get('tab') ?? 'monitor') === 'fleet' ? 'fleet' : 'monitor'
	);

	function selectTab(tab: 'monitor' | 'fleet'): void {
		void goto(resolve(`/steering?tab=${tab}`), {
			replace: true,
			reset: false
		});
	}
</script>

<svelte:head>
	<title>Steering — mike</title>
</svelte:head>

<div class="sh-page" use:revealSections>
	<header class="sh-intro" data-reveal>
		<PageIntro level="h1" title="Steering" learnMoreHref="/help/guides/steering-concepts">
			<span>
				Attach cheap watcher agents, called steerers<HelpHint slug="steerer" term="steerers" />, to
				a live session. After each turn a steerer evaluates drift and, when the work wanders off
				track, sends a first-class corrective, a steered turn<HelpHint
					slug="steered-turn"
					term="steered turn"
				/>, attributed to the steerer, never disguised as the operator. The <strong>Monitor</strong>
				tab shows live fires across sessions; the <strong>Fleet</strong><HelpHint
					slug="fleet"
					term="fleet"
				/> tab manages the roster and enrolments<HelpHint
					slug="steering-enrolment"
					term="enrolments"
				/>.
			</span>
		</PageIntro>
	</header>

	<Tabs.Root
		value={activeTab}
		onValueChange={(value) => selectTab(value === 'fleet' ? 'fleet' : 'monitor')}
		class="min-h-0 flex-1 gap-0"
	>
		<div class="sh-tabbar" data-reveal>
			<Tabs.List aria-label="Steering sections">
				<Tabs.Trigger value="monitor">
					<span>Monitor<span class="sh-tab-count"> · {liveStore.enrolledSessionCount}</span></span>
				</Tabs.Trigger>
				<Tabs.Trigger value="fleet">
					<span
						>Fleet{#if fleetCount !== null}<span class="sh-tab-count">
								· {fleetCount}</span
							>{/if}</span
					>
				</Tabs.Trigger>
			</Tabs.List>
		</div>
		<Tabs.Content value="monitor" class="min-h-0 overflow-auto">
			<div class="sh-panel" in:fade={{ duration: 120 }}>
				<SteeringMonitor {now} store={liveStore} />
			</div>
		</Tabs.Content>
		<Tabs.Content value="fleet" class="min-h-0 overflow-auto">
			<div class="sh-panel" in:fade={{ duration: 120 }}>
				<SteererFleet store={liveStore} onCount={(n) => (fleetCount = n)} />
			</div>
		</Tabs.Content>
	</Tabs.Root>
</div>

<style>
	.sh-page {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
		overflow: hidden;
	}

	.sh-intro {
		flex-shrink: 0;
		padding: var(--space-5) var(--space-5) var(--space-4);
	}

	.sh-tabbar {
		padding: 0 var(--space-5) var(--space-3);
	}
	.sh-panel {
		min-height: 0;
		height: 100%;
	}
	.sh-tab-count {
		color: var(--text-3);
		font-weight: 400;
	}
</style>
