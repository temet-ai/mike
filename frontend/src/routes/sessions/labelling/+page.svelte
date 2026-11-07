<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import * as Tabs from '$lib/components/ui/tabs';
	import JudgeHome from '$lib/components/judges/JudgeHome.svelte';
	import type { JudgePageData } from '$lib/components/judges/judgePageData';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';

	let { data }: { data: JudgePageData } = $props();

	const tab = $derived(data.role === 'session_judge' ? 'sessions' : 'turns');

	function pick(value: string) {
		if (value === tab) return;
		const base = resolve('/sessions/labelling');
		void goto(value === 'sessions' ? `${base}?for=sessions` : base, {
			reset: false
		});
	}
</script>

<svelte:head><title>Judge — Sessions — mike</title></svelte:head>

{#key data.role}
	<JudgeHome {data}>
		{#snippet head()}
			<PageIntro level="h1" title="Judge">
				An AI model, the judge, labels your finished turns and sessions. Here you see whether it is
				working and how accurate it is, and change how it runs. Its labels lead to lessons and nudges
				in <a class="lb-link" href={resolve('/sessions/improvement')}>What the judge did</a>.
			</PageIntro>
			<Tabs.Root value={tab} onValueChange={pick}>
				<Tabs.List aria-label="What is labelled">
					<Tabs.Trigger value="turns">Turns</Tabs.Trigger>
					<Tabs.Trigger value="sessions">Sessions</Tabs.Trigger>
				</Tabs.List>
			</Tabs.Root>
		{/snippet}
	</JudgeHome>
{/key}

<style>
	.lb-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.lb-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
