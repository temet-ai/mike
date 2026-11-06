<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import * as Tabs from './index';

	const { Story } = defineMeta({
		title: 'UI/Tabs',
		component: Tabs.Root,
		tags: ['autodocs']
	});
</script>

<Story name="Default">
	{#snippet template()}
		<Tabs.Root value="overview" class="w-96">
			<Tabs.List>
				<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
				<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
				<Tabs.Trigger value="analysis">Analysis</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]">
				Overview content
			</Tabs.Content>
			<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]">
				Requests content
			</Tabs.Content>
			<Tabs.Content value="analysis" class="pt-4 text-[var(--text-1)]">
				Analysis content
			</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="WithDisabledTrigger">
	{#snippet template()}
		<Tabs.Root value="active" class="w-96">
			<Tabs.List>
				<Tabs.Trigger value="active">Active</Tabs.Trigger>
				<Tabs.Trigger value="pending" disabled>Pending</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="active" class="pt-4 text-[var(--text-1)]">Active tab body</Tabs.Content>
			<Tabs.Content value="pending" class="pt-4 text-[var(--text-1)]">Pending tab body</Tabs.Content
			>
		</Tabs.Root>
	{/snippet}
</Story>
