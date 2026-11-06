<!--
@component
The per-project choice of whether mike may write skills into the project on its own, or only
after the user approves each one. Saves through `onChange`; shows the save error in place.
-->
<script lang="ts">
	import * as RadioGroup from '$lib/components/ui/radio-group';
	import type { SkillMode } from '$lib/api/skills';

	let {
		mode,
		projectName,
		onChange
	}: {
		mode: SkillMode;
		projectName: string;
		onChange: (mode: SkillMode) => Promise<void>;
	} = $props();

	// svelte-ignore state_referenced_locally -- seeded once; the control owns the value while it saves
	let value = $state<string>(mode);
	// svelte-ignore state_referenced_locally -- the last saved choice, to fall back to on failure
	let saved = $state<string>(mode);
	let saving = $state(false);
	let error = $state<string | null>(null);

	async function choose(next: string): Promise<void> {
		saving = true;
		error = null;
		try {
			await onChange(next as SkillMode);
			saved = next;
		} catch (e) {
			value = saved;
			error = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}
</script>

<div class="smc" data-slot="skill-mode-control">
	<RadioGroup.Root
		name="skill-mode"
		bind:value
		disabled={saving}
		onValueChange={choose}
		aria-label={`How mike ships skills to ${projectName}`}
	>
		<RadioGroup.Item
			value="ask"
			label="Ask me first"
			description="mike writes a skill into this project only after you approve it here."
		/>
		<RadioGroup.Item
			value="auto"
			label="Ship when it passes review"
			description="mike writes every skill that passes its own review, and a plain list of the lessons when there is no reviewed draft. You can still reject or retire any of them."
		/>
	</RadioGroup.Root>
	<p class="smc-error" role="alert" aria-live="polite">{error ?? ''}</p>
</div>

<style>
	.smc {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.smc-error {
		margin: 0;
		min-height: 1lh;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
</style>
