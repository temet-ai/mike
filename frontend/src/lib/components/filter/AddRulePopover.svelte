<script lang="ts">
	import type { FilterRuleDto } from '$lib/types';
	import type { Snippet } from 'svelte';
	import * as Popover from '$lib/components/ui/popover';
	import FilterRuleForm from './FilterRuleForm.svelte';

	interface TriggerSnippetArgs {
		props: Record<string, unknown>;
	}

	interface Props {
		initialRule: FilterRuleDto;
		triggerLabel: string;
		trigger: Snippet<[TriggerSnippetArgs]>;
	}

	let { initialRule, triggerLabel, trigger }: Props = $props();

	let open = $state(false);

	function closePopover(): void {
		open = false;
	}
</script>

<Popover.Root bind:open>
	<Popover.Trigger aria-label={triggerLabel}>
		{#snippet child({ props }: TriggerSnippetArgs)}
			{@render trigger({ props })}
		{/snippet}
	</Popover.Trigger>
	<Popover.Content class="w-[22rem]">
		<FilterRuleForm
			{initialRule}
			density="popover"
			onCancel={closePopover}
			onComplete={closePopover}
		/>
	</Popover.Content>
</Popover.Root>
