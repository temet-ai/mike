<!--
@component
Loads the open alarms and shows the ones that belong on this page (`home`), as plain sentences with
Acknowledge. Settings pages that operate a subject place it where the operator acts on that subject,
so an alarm sits next to the thing it is about. Renders nothing when this page has no alarms.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { acknowledgeEvalAlarm, getEvalAlarms } from '$lib/api/evalMetrics';
	import type { EvalAlarmDto } from '$lib/api/evalMetrics';
	import { listEvalBundles } from '$lib/api/evalRuns';
	import type { BundleDto } from '$lib/api/evalRuns';
	import { alarmsFor } from '$lib/components/judges/judges';
	import type { AlarmHome } from '$lib/components/judges/judges';
	import EvalAlarmList from './EvalAlarmList.svelte';

	let { home }: { home: AlarmHome } = $props();

	let alarms = $state<EvalAlarmDto[]>([]);
	let bundles = $state<BundleDto[]>([]);
	let error = $state<string | null>(null);

	const mine = $derived(alarmsFor(alarms, bundles, home));

	async function load() {
		try {
			const [open, list] = await Promise.all([getEvalAlarms({ state: 'open' }), listEvalBundles()]);
			alarms = open.alarms;
			bundles = list.bundles;
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}

	onMount(() => {
		void load();
	});

	async function acknowledge(id: number, note: string) {
		await acknowledgeEvalAlarm(id, note);
		await load();
	}
</script>

<EvalAlarmList alarms={mine} {error} onAcknowledge={acknowledge} />
