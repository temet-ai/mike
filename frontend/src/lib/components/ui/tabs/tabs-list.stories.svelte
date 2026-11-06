<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, expect, userEvent, within, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Tabs from './index';

	const { Story } = defineMeta({
		title: 'UI/Tabs/TabsList',
		component: Tabs.List,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded'
		}
	});

	const dataSlotAttributeValueChangeSpy = fn();
	const customClassForwardedValueChangeSpy = fn();
	const valueChangeCallbackSpy = fn();
	const disabledTriggerNoCallbackSpy = fn();
</script>

<Story name="Default" tags={['design-exploration']}>
	{#snippet template()}
		<Tabs.Root value="overview" class="w-96">
			<Tabs.List>
				<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
				<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
				<Tabs.Trigger value="analysis">Analysis</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]"
				>Overview content</Tabs.Content
			>
			<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]"
				>Requests content</Tabs.Content
			>
			<Tabs.Content value="analysis" class="pt-4 text-[var(--text-1)]"
				>Analysis content</Tabs.Content
			>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="TwoTabs" tags={['design-exploration']}>
	{#snippet template()}
		<Tabs.Root value="sessions" class="w-72">
			<Tabs.List>
				<Tabs.Trigger value="sessions">Sessions</Tabs.Trigger>
				<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="sessions" class="pt-4 text-[var(--text-1)]">Sessions pane</Tabs.Content>
			<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]">Requests pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="ManyTabs" tags={['design-exploration']}>
	{#snippet template()}
		<Tabs.Root value="messages" class="w-full max-w-2xl">
			<Tabs.List>
				<Tabs.Trigger value="messages">Messages</Tabs.Trigger>
				<Tabs.Trigger value="tools">Tools</Tabs.Trigger>
				<Tabs.Trigger value="system">System</Tabs.Trigger>
				<Tabs.Trigger value="raw">Raw</Tabs.Trigger>
				<Tabs.Trigger value="meta">Meta</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="messages" class="pt-4 text-[var(--text-1)]">Messages</Tabs.Content>
			<Tabs.Content value="tools" class="pt-4 text-[var(--text-1)]">Tools</Tabs.Content>
			<Tabs.Content value="system" class="pt-4 text-[var(--text-1)]">System</Tabs.Content>
			<Tabs.Content value="raw" class="pt-4 text-[var(--text-1)]">Raw</Tabs.Content>
			<Tabs.Content value="meta" class="pt-4 text-[var(--text-1)]">Meta</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="WithDisabled" tags={['design-exploration']}>
	{#snippet template()}
		<Tabs.Root value="active" class="w-72">
			<Tabs.List>
				<Tabs.Trigger value="active">Active</Tabs.Trigger>
				<Tabs.Trigger value="locked" disabled>Locked</Tabs.Trigger>
				<Tabs.Trigger value="pending">Pending</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="active" class="pt-4 text-[var(--text-1)]">Active pane</Tabs.Content>
			<Tabs.Content value="locked" class="pt-4 text-[var(--text-1)]">Locked pane</Tabs.Content>
			<Tabs.Content value="pending" class="pt-4 text-[var(--text-1)]">Pending pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="Vertical" tags={['design-exploration']}>
	{#snippet template()}
		<Tabs.Root value="sessions" orientation="vertical" class="w-64 flex-row">
			<Tabs.List class="border-b-0 border-r border-border h-fit flex-col pr-4">
				<Tabs.Trigger value="sessions">Sessions</Tabs.Trigger>
				<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
				<Tabs.Trigger value="analysis">Analysis</Tabs.Trigger>
			</Tabs.List>
			<div class="flex-1 pl-4">
				<Tabs.Content value="sessions" class="text-[var(--text-1)]">Sessions</Tabs.Content>
				<Tabs.Content value="requests" class="text-[var(--text-1)]">Requests</Tabs.Content>
				<Tabs.Content value="analysis" class="text-[var(--text-1)]">Analysis</Tabs.Content>
			</div>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="RefBindable"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const onValueChange = (globalThis as Record<string, unknown>).__refBindableValueChangeSpy as
			ReturnType<typeof fn> | undefined;

		const listEl = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="tabs-list"]') as HTMLElement | null;
			if (!el) throw new Error('TabsList [data-slot="tabs-list"] element must be in the DOM');
			return el;
		});
		await expect(listEl, 'TabsList list element must be in the DOM').toBeTruthy();
		await expect(listEl.tagName.toLowerCase()).toBe('div');

		await expect(listEl.getAttribute('data-slot')).toBe('tabs-list');

		const detailTab = canvas.getByRole('tab', { name: 'Detail' });
		await userEvent.click(detailTab);
		await expect(onValueChange).toBeDefined();
		await expect(onValueChange).toHaveBeenCalledWith('detail');
	}}
>
	{#snippet template()}
		{@const valueChangeSpy = fn()}
		{@const _store = (() => {
			(globalThis as Record<string, unknown>).__refBindableValueChangeSpy = valueChangeSpy;
		})()}
		<Tabs.Root value="overview" class="w-72" onValueChange={valueChangeSpy}>
			<Tabs.List>
				<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
				<Tabs.Trigger value="detail">Detail</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]">Overview pane</Tabs.Content>
			<Tabs.Content value="detail" class="pt-4 text-[var(--text-1)]">Detail pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="SnapshotVertical"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-list/Vertical');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="sessions" orientation="vertical" class="w-64 flex-row">
				<Tabs.List class="border-b-0 border-r border-border h-fit flex-col pr-4">
					<Tabs.Trigger value="sessions">Sessions</Tabs.Trigger>
					<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
					<Tabs.Trigger value="analysis">Analysis</Tabs.Trigger>
				</Tabs.List>
				<div class="flex-1 pl-4">
					<Tabs.Content value="sessions" class="text-[var(--text-1)]">Sessions</Tabs.Content>
					<Tabs.Content value="requests" class="text-[var(--text-1)]">Requests</Tabs.Content>
					<Tabs.Content value="analysis" class="text-[var(--text-1)]">Analysis</Tabs.Content>
				</div>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story name="CustomClass" tags={['design-exploration']}>
	{#snippet template()}
		<Tabs.Root value="tab1" class="w-72">
			<Tabs.List class="gap-8">
				<Tabs.Trigger value="tab1">Tab One</Tabs.Trigger>
				<Tabs.Trigger value="tab2">Tab Two</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="tab1" class="pt-4 text-[var(--text-1)]">Tab one content</Tabs.Content>
			<Tabs.Content value="tab2" class="pt-4 text-[var(--text-1)]">Tab two content</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="AllThemes" tags={['design-exploration']}>
	{#snippet template()}
		<div style="display: flex; flex-direction: column; gap: 2rem; padding: 1rem;">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div
					class={theme}
					style="background: var(--surface-1, var(--bg-base)); border-radius: 0.5rem; padding: 1rem;"
				>
					<p
						style="font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.5rem; color: var(--muted-foreground);"
					>
						{theme}
					</p>
					<Tabs.Root value="active" class="w-64">
						<Tabs.List>
							<Tabs.Trigger value="active">Active</Tabs.Trigger>
							<Tabs.Trigger value="inactive">Inactive</Tabs.Trigger>
							<Tabs.Trigger value="locked" disabled>Disabled</Tabs.Trigger>
						</Tabs.List>
						<Tabs.Content value="active" class="pt-3 text-[var(--text-1)] text-sm"
							>Active tab content</Tabs.Content
						>
						<Tabs.Content value="inactive" class="pt-3 text-[var(--text-1)] text-sm"
							>Inactive tab content</Tabs.Content
						>
						<Tabs.Content value="locked" class="pt-3 text-[var(--text-1)] text-sm"
							>Locked tab content</Tabs.Content
						>
					</Tabs.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="DataSlotAttribute"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		dataSlotAttributeValueChangeSpy.mockClear();
		const canvas = within(canvasElement);

		const list = canvasElement.querySelector('[data-slot="tabs-list"]');
		await expect(list, 'TabsList should render with data-slot="tabs-list"').toBeTruthy();

		const triggers = list?.querySelectorAll('[role="tab"]');
		await expect(triggers?.length, 'list should contain 2 tab triggers').toBe(2);

		const requestsTab = canvas.getByRole('tab', { name: 'Requests' });
		await userEvent.click(requestsTab);
		await expect(dataSlotAttributeValueChangeSpy).toHaveBeenCalledWith('requests');
	}}
>
	{#snippet template()}
		<Tabs.Root value="sessions" class="w-72" onValueChange={dataSlotAttributeValueChangeSpy}>
			<Tabs.List>
				<Tabs.Trigger value="sessions">Sessions</Tabs.Trigger>
				<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="sessions" class="pt-4 text-[var(--text-1)]">Sessions pane</Tabs.Content>
			<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]">Requests pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="CustomClassForwarded"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		customClassForwardedValueChangeSpy.mockClear();
		const canvas = within(canvasElement);

		const list = canvasElement.querySelector('[data-slot="tabs-list"]');
		await expect(list, 'TabsList element should be in the DOM').toBeTruthy();
		await expect((list as HTMLElement).className).toContain('gap-8');

		const tabTwo = canvas.getByRole('tab', { name: 'Tab Two' });
		await userEvent.click(tabTwo);
		await expect(customClassForwardedValueChangeSpy).toHaveBeenCalledWith('tab2');
	}}
>
	{#snippet template()}
		<Tabs.Root value="tab1" class="w-72" onValueChange={customClassForwardedValueChangeSpy}>
			<Tabs.List class="gap-8">
				<Tabs.Trigger value="tab1">Tab One</Tabs.Trigger>
				<Tabs.Trigger value="tab2">Tab Two</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="tab1" class="pt-4 text-[var(--text-1)]">Tab one</Tabs.Content>
			<Tabs.Content value="tab2" class="pt-4 text-[var(--text-1)]">Tab two</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="ValueChangeCallback"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		valueChangeCallbackSpy.mockClear();
		const canvas = within(canvasElement);

		const messagesTab = canvas.getByRole('tab', { name: 'Messages' });
		const toolsTab = canvas.getByRole('tab', { name: 'Tools' });
		const systemTab = canvas.getByRole('tab', { name: 'System' });

		await expect(messagesTab).toHaveAttribute('data-state', 'active');
		await expect(toolsTab).toHaveAttribute('data-state', 'inactive');

		await userEvent.click(toolsTab);
		await expect(valueChangeCallbackSpy).toHaveBeenCalledWith('tools');
		await expect(toolsTab).toHaveAttribute('data-state', 'active');
		await expect(messagesTab).toHaveAttribute('data-state', 'inactive');

		await userEvent.click(systemTab);
		await expect(valueChangeCallbackSpy).toHaveBeenCalledWith('system');
		await expect(systemTab).toHaveAttribute('data-state', 'active');
		await expect(toolsTab).toHaveAttribute('data-state', 'inactive');
	}}
>
	{#snippet template()}
		<Tabs.Root value="messages" class="w-72" onValueChange={valueChangeCallbackSpy}>
			<Tabs.List>
				<Tabs.Trigger value="messages">Messages</Tabs.Trigger>
				<Tabs.Trigger value="tools">Tools</Tabs.Trigger>
				<Tabs.Trigger value="system">System</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="messages" class="pt-4 text-[var(--text-1)]">Messages pane</Tabs.Content>
			<Tabs.Content value="tools" class="pt-4 text-[var(--text-1)]">Tools pane</Tabs.Content>
			<Tabs.Content value="system" class="pt-4 text-[var(--text-1)]">System pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="DisabledTriggerNoCallback"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		disabledTriggerNoCallbackSpy.mockClear();
		const canvas = within(canvasElement);

		const activeTrigger = canvas.getByRole('tab', { name: 'Active' });
		const lockedTrigger = canvas.getByRole('tab', { name: 'Locked' });

		await expect(lockedTrigger).toBeDisabled();

		await expect(activeTrigger).toHaveAttribute('data-state', 'active');
		await expect(lockedTrigger).toHaveAttribute('data-state', 'inactive');

		await expect(disabledTriggerNoCallbackSpy).not.toHaveBeenCalled();

		await expect(lockedTrigger).toBeDisabled();
		await expect(activeTrigger).toHaveAttribute('data-state', 'active');
		await expect(lockedTrigger).toHaveAttribute('data-state', 'inactive');
		await expect(disabledTriggerNoCallbackSpy).not.toHaveBeenCalled();

		await userEvent.click(activeTrigger);
		await expect(disabledTriggerNoCallbackSpy).not.toHaveBeenCalled();
	}}
>
	{#snippet template()}
		<Tabs.Root value="active" class="w-72" onValueChange={disabledTriggerNoCallbackSpy}>
			<Tabs.List>
				<Tabs.Trigger value="active">Active</Tabs.Trigger>
				<Tabs.Trigger value="locked" disabled>Locked</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="active" class="pt-4 text-[var(--text-1)]">Active pane</Tabs.Content>
			<Tabs.Content value="locked" class="pt-4 text-[var(--text-1)]">Locked pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="SnapshotDefault"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-list/Default');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="overview" class="w-96">
				<Tabs.List>
					<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
					<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
					<Tabs.Trigger value="analysis">Analysis</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]"
					>Overview content</Tabs.Content
				>
				<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]"
					>Requests content</Tabs.Content
				>
				<Tabs.Content value="analysis" class="pt-4 text-[var(--text-1)]"
					>Analysis content</Tabs.Content
				>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotActiveIndicator"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-list/ActiveIndicator');
	}}
>
	{#snippet template()}
		<div style="padding: 1.5rem 1rem 2rem;">
			<Tabs.Root value="active" class="w-64">
				<Tabs.List>
					<Tabs.Trigger value="active">Active</Tabs.Trigger>
					<Tabs.Trigger value="inactive">Inactive</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="active" class="pt-4 text-[var(--text-1)] text-sm"
					>Active pane</Tabs.Content
				>
				<Tabs.Content value="inactive" class="pt-4 text-[var(--text-1)] text-sm"
					>Inactive pane</Tabs.Content
				>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotWithDisabled"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-list/WithDisabled');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="active" class="w-72">
				<Tabs.List>
					<Tabs.Trigger value="active">Active</Tabs.Trigger>
					<Tabs.Trigger value="locked" disabled>Locked</Tabs.Trigger>
					<Tabs.Trigger value="pending">Pending</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="active" class="pt-4 text-[var(--text-1)]">Active pane</Tabs.Content>
				<Tabs.Content value="locked" class="pt-4 text-[var(--text-1)]">Locked pane</Tabs.Content>
				<Tabs.Content value="pending" class="pt-4 text-[var(--text-1)]">Pending pane</Tabs.Content>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotAllThemes"
	tags={['visual', 'design-exploration']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-list/AllThemes');
	}}
>
	{#snippet template()}
		<div style="display: flex; flex-direction: column; gap: 2rem; padding: 1rem;">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div
					class={theme}
					style="background: var(--surface-1, var(--bg-base)); border-radius: 0.5rem; padding: 1rem;"
				>
					<p
						style="font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.5rem; color: var(--muted-foreground);"
					>
						{theme}
					</p>
					<Tabs.Root value="active" class="w-64">
						<Tabs.List>
							<Tabs.Trigger value="active">Active</Tabs.Trigger>
							<Tabs.Trigger value="inactive">Inactive</Tabs.Trigger>
							<Tabs.Trigger value="locked" disabled>Disabled</Tabs.Trigger>
						</Tabs.List>
						<Tabs.Content value="active" class="pt-3 text-[var(--text-1)] text-sm"
							>Active tab content</Tabs.Content
						>
						<Tabs.Content value="inactive" class="pt-3 text-[var(--text-1)] text-sm"
							>Inactive tab content</Tabs.Content
						>
						<Tabs.Content value="locked" class="pt-3 text-[var(--text-1)] text-sm"
							>Locked tab content</Tabs.Content
						>
					</Tabs.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
