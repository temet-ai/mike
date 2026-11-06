<script lang="ts">
	import { page } from '$app/state';
	import { UNRESOLVED_PERSONA_SEGMENT } from '$lib/api';
	import PromptClassDetail from '$lib/components/prompts/PromptClassDetail.svelte';

	const rawPersona = $derived(page.params.persona ?? null);
	const persona = $derived(rawPersona === UNRESOLVED_PERSONA_SEGMENT ? null : rawPersona);
</script>

<svelte:head>
	<title>{persona ?? 'Subagent (persona unresolved)'} — Prompts — mike</title>
</svelte:head>

<PromptClassDetail lane="subagent" {persona} />
