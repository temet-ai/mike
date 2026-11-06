<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Tabs from './index';

	const { Story } = defineMeta({
		title: 'UI/Tabs/TabsTrigger',
		component: Tabs.Trigger,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded'
		}
	});
</script>

<script lang="ts">
	let refTarget = $state<HTMLElement | null>(null);
</script>

<Story name="Inactive">
	{#snippet template()}
		<Tabs.Root value="other" class="w-72">
			<Tabs.List>
				<Tabs.Trigger value="tab1">Sessions</Tabs.Trigger>
				<Tabs.Trigger value="other">Requests</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="tab1" class="pt-4 text-[var(--text-1)]">Sessions pane</Tabs.Content>
			<Tabs.Content value="other" class="pt-4 text-[var(--text-1)]">Requests pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="Active">
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

<Story name="Disabled">
	{#snippet template()}
		<Tabs.Root value="active" class="w-72">
			<Tabs.List>
				<Tabs.Trigger value="active">Active</Tabs.Trigger>
				<Tabs.Trigger value="locked" disabled>Locked</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="active" class="pt-4 text-[var(--text-1)]">Active pane</Tabs.Content>
			<Tabs.Content value="locked" class="pt-4 text-[var(--text-1)]">Locked pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="TextOnly">
	{#snippet template()}
		<Tabs.Root value="overview" class="w-72">
			<Tabs.List>
				<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
				<Tabs.Trigger value="detail">Detail</Tabs.Trigger>
				<Tabs.Trigger value="raw">Raw</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]">Overview</Tabs.Content>
			<Tabs.Content value="detail" class="pt-4 text-[var(--text-1)]">Detail</Tabs.Content>
			<Tabs.Content value="raw" class="pt-4 text-[var(--text-1)]">Raw</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="IconAndText">
	{#snippet template()}
		<Tabs.Root value="sessions" class="w-72">
			<Tabs.List>
				<Tabs.Trigger value="sessions">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
					>
						<rect width="18" height="18" x="3" y="3" rx="2" /><path d="M7 7h10M7 12h10M7 17h4" />
					</svg>
					Sessions
				</Tabs.Trigger>
				<Tabs.Trigger value="analysis">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
					>
						<path d="M3 3v18h18" /><path d="m19 9-5 5-4-4-3 3" />
					</svg>
					Analysis
				</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="sessions" class="pt-4 text-[var(--text-1)]">Sessions</Tabs.Content>
			<Tabs.Content value="analysis" class="pt-4 text-[var(--text-1)]">Analysis</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="IconOnly">
	{#snippet template()}
		<Tabs.Root value="grid" class="w-40">
			<Tabs.List>
				<Tabs.Trigger value="grid" aria-label="Grid view">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
					>
						<rect width="7" height="7" x="3" y="3" rx="1" />
						<rect width="7" height="7" x="14" y="3" rx="1" />
						<rect width="7" height="7" x="14" y="14" rx="1" />
						<rect width="7" height="7" x="3" y="14" rx="1" />
					</svg>
				</Tabs.Trigger>
				<Tabs.Trigger value="list" aria-label="List view">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
					>
						<line x1="8" x2="21" y1="6" y2="6" />
						<line x1="8" x2="21" y1="12" y2="12" />
						<line x1="8" x2="21" y1="18" y2="18" />
						<line x1="3" x2="3.01" y1="6" y2="6" />
						<line x1="3" x2="3.01" y1="12" y2="12" />
						<line x1="3" x2="3.01" y1="18" y2="18" />
					</svg>
				</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="grid" class="pt-4 text-[var(--text-1)]">Grid view</Tabs.Content>
			<Tabs.Content value="list" class="pt-4 text-[var(--text-1)]">List view</Tabs.Content>
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
						<Tabs.Content value="active" class="pt-3 text-[var(--text-1)] text-sm">
							Active tab content
						</Tabs.Content>
						<Tabs.Content value="inactive" class="pt-3 text-[var(--text-1)] text-sm">
							Inactive tab content
						</Tabs.Content>
						<Tabs.Content value="locked" class="pt-3 text-[var(--text-1)] text-sm">
							Locked tab content
						</Tabs.Content>
					</Tabs.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="ClickToSwitch"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const sessionsTrigger = canvas.getByRole('tab', { name: 'Sessions' });
		const requestsTrigger = canvas.getByRole('tab', { name: 'Requests' });

		await expect(sessionsTrigger).toHaveAttribute('data-state', 'active');
		await expect(requestsTrigger).toHaveAttribute('data-state', 'inactive');

		await userEvent.click(requestsTrigger);

		await expect(requestsTrigger).toHaveAttribute('data-state', 'active');
		await expect(sessionsTrigger).toHaveAttribute('data-state', 'inactive');
	}}
>
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

<Story
	name="KeyboardNavigation"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const overviewTab = canvas.getByRole('tab', { name: 'Overview' });
		const detailTab = canvas.getByRole('tab', { name: 'Detail' });
		const rawTab = canvas.getByRole('tab', { name: 'Raw' });

		await userEvent.click(overviewTab);
		await expect(overviewTab).toHaveAttribute('data-state', 'active');

		await userEvent.keyboard('{ArrowRight}');
		await expect(document.activeElement).toBe(detailTab);

		await userEvent.keyboard('{ArrowRight}');
		await expect(document.activeElement).toBe(rawTab);

		await userEvent.keyboard('{Enter}');
		await expect(rawTab).toHaveAttribute('data-state', 'active');
		await expect(overviewTab).toHaveAttribute('data-state', 'inactive');

		await userEvent.keyboard('{ArrowLeft}');
		await expect(document.activeElement).toBe(detailTab);
	}}
>
	{#snippet template()}
		<Tabs.Root value="overview" class="w-72">
			<Tabs.List>
				<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
				<Tabs.Trigger value="detail">Detail</Tabs.Trigger>
				<Tabs.Trigger value="raw">Raw</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]">Overview</Tabs.Content>
			<Tabs.Content value="detail" class="pt-4 text-[var(--text-1)]">Detail</Tabs.Content>
			<Tabs.Content value="raw" class="pt-4 text-[var(--text-1)]">Raw</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="DisabledNoOp"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const activeTrigger = canvas.getByRole('tab', { name: 'Active' });
		const lockedTrigger = canvas.getByRole('tab', { name: 'Locked' });

		await expect(lockedTrigger).toBeDisabled();

		await expect(activeTrigger).toHaveAttribute('data-state', 'active');
		await expect(lockedTrigger).toHaveAttribute('data-state', 'inactive');

		await expect(lockedTrigger).toBeDisabled();
		await expect(activeTrigger).toHaveAttribute('data-state', 'active');
		await expect(lockedTrigger).toHaveAttribute('data-state', 'inactive');
	}}
>
	{#snippet template()}
		<Tabs.Root value="active" class="w-72">
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
	name="ActiveStateAttributes"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const messagesTab = canvas.getByRole('tab', { name: 'Messages' });
		const toolsTab = canvas.getByRole('tab', { name: 'Tools' });
		const systemTab = canvas.getByRole('tab', { name: 'System' });

		await expect(messagesTab).toHaveAttribute('data-state', 'active');
		await expect(toolsTab).toHaveAttribute('data-state', 'inactive');
		await expect(systemTab).toHaveAttribute('data-state', 'inactive');

		await userEvent.click(toolsTab);
		await expect(toolsTab).toHaveAttribute('data-state', 'active');
		await expect(messagesTab).toHaveAttribute('data-state', 'inactive');
		await expect(systemTab).toHaveAttribute('data-state', 'inactive');

		await userEvent.click(systemTab);
		await expect(systemTab).toHaveAttribute('data-state', 'active');
		await expect(messagesTab).toHaveAttribute('data-state', 'inactive');
		await expect(toolsTab).toHaveAttribute('data-state', 'inactive');
	}}
>
	{#snippet template()}
		<Tabs.Root value="messages" class="w-72">
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
	name="CustomClass"
	tags={['interactive', 'design-exploration']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const customTrigger = canvas.getByRole('tab', { name: 'Custom' });
		await expect(customTrigger).toHaveClass('px-4');
	}}
>
	{#snippet template()}
		<Tabs.Root value="custom" class="w-64">
			<Tabs.List>
				<Tabs.Trigger value="custom" class="px-4">Custom</Tabs.Trigger>
				<Tabs.Trigger value="normal">Normal</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="custom" class="pt-4 text-[var(--text-1)]">Custom pane</Tabs.Content>
			<Tabs.Content value="normal" class="pt-4 text-[var(--text-1)]">Normal pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="HoverInactive"
	tags={['interactive']}
	args={{ onmouseenter: fn() }}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);

		const inactiveTrigger = canvas.getByRole('tab', { name: 'Requests' });
		const activeTrigger = canvas.getByRole('tab', { name: 'Sessions' });

		await expect(inactiveTrigger).toHaveAttribute('data-state', 'inactive');

		await userEvent.hover(inactiveTrigger);

		await expect(args.onmouseenter).toHaveBeenCalled();

		await expect(inactiveTrigger).toHaveAttribute('data-state', 'inactive');
		await expect(activeTrigger).toHaveAttribute('data-state', 'active');

		await userEvent.unhover(inactiveTrigger);
		await expect(inactiveTrigger).toHaveAttribute('data-state', 'inactive');
	}}
>
	{#snippet template(args)}
		<Tabs.Root value="sessions" class="w-72">
			<Tabs.List>
				<Tabs.Trigger value="sessions">Sessions</Tabs.Trigger>
				<Tabs.Trigger value="requests" onmouseenter={args.onmouseenter}>Requests</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="sessions" class="pt-4 text-[var(--text-1)]">Sessions pane</Tabs.Content>
			<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]">Requests pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="FocusVisible"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const overviewTab = canvas.getByRole('tab', { name: 'Overview' });

		await userEvent.click(overviewTab);
		await expect(overviewTab).toHaveAttribute('data-state', 'active');
		await expect(document.activeElement).toBe(overviewTab);

		await userEvent.tab();
		await expect(document.activeElement).not.toBe(overviewTab);

		await userEvent.tab({ shift: true });
		await expect(document.activeElement).toBe(overviewTab);
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="overview" class="w-72">
				<Tabs.List>
					<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
					<Tabs.Trigger value="detail">Detail</Tabs.Trigger>
					<Tabs.Trigger value="raw">Raw</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]">Overview</Tabs.Content>
				<Tabs.Content value="detail" class="pt-4 text-[var(--text-1)]">Detail</Tabs.Content>
				<Tabs.Content value="raw" class="pt-4 text-[var(--text-1)]">Raw</Tabs.Content>
			</Tabs.Root>
			<button class="mt-4 px-2 py-1 text-sm text-[var(--text-1)]" type="button">
				Next focusable (for Tab navigation)
			</button>
		</div>
	{/snippet}
</Story>

<Story
	name="RefForwarding"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const sessionsTrigger = canvas.getByRole('tab', { name: 'Sessions' });

		await expect(sessionsTrigger).toHaveAttribute('data-slot', 'tabs-trigger');

		await expect(sessionsTrigger.tagName.toLowerCase()).toBe('button');

		await expect(sessionsTrigger).toHaveAttribute('role', 'tab');
	}}
>
	{#snippet template()}
		<Tabs.Root value="sessions" class="w-72">
			<Tabs.List>
				<Tabs.Trigger value="sessions" bind:ref={refTarget}>Sessions</Tabs.Trigger>
				<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="sessions" class="pt-4 text-[var(--text-1)]">Sessions pane</Tabs.Content>
			<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]">Requests pane</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="SnapshotHoverInactive"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const inactiveTrigger = canvas.getByRole('tab', { name: 'Requests' });
		await userEvent.hover(inactiveTrigger);

		await captureStorySnapshot(canvasElement, 'tabs-trigger/HoverInactive');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="sessions" class="w-72">
				<Tabs.List>
					<Tabs.Trigger value="sessions">Sessions</Tabs.Trigger>
					<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="sessions" class="pt-4 text-[var(--text-1)]">Sessions pane</Tabs.Content
				>
				<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]">Requests pane</Tabs.Content
				>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotFocusVisible"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const overviewTab = canvas.getByRole('tab', { name: 'Overview' });
		await userEvent.click(overviewTab);

		await userEvent.tab();
		await userEvent.tab({ shift: true });

		await captureStorySnapshot(canvasElement, 'tabs-trigger/FocusVisible');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="overview" class="w-72">
				<Tabs.List>
					<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
					<Tabs.Trigger value="detail">Detail</Tabs.Trigger>
					<Tabs.Trigger value="raw">Raw</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]">Overview</Tabs.Content>
				<Tabs.Content value="detail" class="pt-4 text-[var(--text-1)]">Detail</Tabs.Content>
				<Tabs.Content value="raw" class="pt-4 text-[var(--text-1)]">Raw</Tabs.Content>
			</Tabs.Root>
			<button class="mt-4 px-2 py-1 text-sm text-[var(--text-1)]" type="button">
				Next focusable (for Tab navigation)
			</button>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotInactive"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-trigger/Inactive');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="other" class="w-72">
				<Tabs.List>
					<Tabs.Trigger value="tab1">Sessions</Tabs.Trigger>
					<Tabs.Trigger value="tab2">Requests</Tabs.Trigger>
					<Tabs.Trigger value="other">Analysis</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="tab1" class="pt-4 text-[var(--text-1)]">Sessions pane</Tabs.Content>
				<Tabs.Content value="tab2" class="pt-4 text-[var(--text-1)]">Requests pane</Tabs.Content>
				<Tabs.Content value="other" class="pt-4 text-[var(--text-1)]">Analysis pane</Tabs.Content>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotActive"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-trigger/Active');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="sessions" class="w-72">
				<Tabs.List>
					<Tabs.Trigger value="sessions">Sessions</Tabs.Trigger>
					<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
					<Tabs.Trigger value="analysis">Analysis</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="sessions" class="pt-4 text-[var(--text-1)]">Sessions pane</Tabs.Content
				>
				<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]">Requests pane</Tabs.Content
				>
				<Tabs.Content value="analysis" class="pt-4 text-[var(--text-1)]">Analysis pane</Tabs.Content
				>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotDisabled"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-trigger/Disabled');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="active" class="w-72">
				<Tabs.List>
					<Tabs.Trigger value="active">Active</Tabs.Trigger>
					<Tabs.Trigger value="locked" disabled>Disabled</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="active" class="pt-4 text-[var(--text-1)]">Active pane</Tabs.Content>
				<Tabs.Content value="locked" class="pt-4 text-[var(--text-1)]">Locked pane</Tabs.Content>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotIconAndText"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-trigger/IconAndText');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="sessions" class="w-72">
				<Tabs.List>
					<Tabs.Trigger value="sessions">
						<svg
							xmlns="http://www.w3.org/2000/svg"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
						>
							<rect width="18" height="18" x="3" y="3" rx="2" />
							<path d="M7 7h10M7 12h10M7 17h4" />
						</svg>
						Sessions
					</Tabs.Trigger>
					<Tabs.Trigger value="analysis">
						<svg
							xmlns="http://www.w3.org/2000/svg"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
						>
							<path d="M3 3v18h18" /><path d="m19 9-5 5-4-4-3 3" />
						</svg>
						Analysis
					</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="sessions" class="pt-4 text-[var(--text-1)]">Sessions</Tabs.Content>
				<Tabs.Content value="analysis" class="pt-4 text-[var(--text-1)]">Analysis</Tabs.Content>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotAllThemes"
	tags={['visual', 'design-exploration']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-trigger/AllThemes');
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
							>Active</Tabs.Content
						>
						<Tabs.Content value="inactive" class="pt-3 text-[var(--text-1)] text-sm"
							>Inactive</Tabs.Content
						>
						<Tabs.Content value="locked" class="pt-3 text-[var(--text-1)] text-sm"
							>Locked</Tabs.Content
						>
					</Tabs.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
