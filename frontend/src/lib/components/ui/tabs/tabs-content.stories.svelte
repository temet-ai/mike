<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Tabs from './index';

	const { Story } = defineMeta({
		title: 'UI/Tabs/TabsContent',
		component: Tabs.Content,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded'
		}
	});

	const tabSwitchingValueChangeSpy = fn();
	const keyboardWithinContentValueChangeSpy = fn();
</script>

<Story name="Default">
	{#snippet template()}
		<Tabs.Root value="overview" class="w-80">
			<Tabs.List>
				<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
				<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]">
				Overview content
			</Tabs.Content>
			<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]">
				Requests content
			</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="EmptyContent">
	{#snippet template()}
		<Tabs.Root value="empty" class="w-80">
			<Tabs.List>
				<Tabs.Trigger value="empty">Empty</Tabs.Trigger>
				<Tabs.Trigger value="other">Other</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="empty" class="pt-4" />
			<Tabs.Content value="other" class="pt-4 text-[var(--text-1)]">Other content</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="WithTextContent">
	{#snippet template()}
		<Tabs.Root value="text" class="w-80">
			<Tabs.List>
				<Tabs.Trigger value="text">Text</Tabs.Trigger>
				<Tabs.Trigger value="other">Other</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="text" class="pt-4 text-[var(--text-1)]">
				This is a plain text paragraph rendered inside the active tab panel.
			</Tabs.Content>
			<Tabs.Content value="other" class="pt-4 text-[var(--text-1)]">Other panel</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="WithNestedHTML">
	{#snippet template()}
		<Tabs.Root value="detail" class="w-96">
			<Tabs.List>
				<Tabs.Trigger value="detail">Detail</Tabs.Trigger>
				<Tabs.Trigger value="raw">Raw</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="detail" class="pt-4 text-[var(--text-1)]">
				<div style="display: flex; flex-direction: column; gap: 0.75rem;">
					<h3 style="font-weight: 600; font-size: 0.875rem;">Request Detail</h3>
					<ul
						style="list-style: disc; padding-left: 1.25rem; font-size: 0.875rem; display: flex; flex-direction: column; gap: 0.25rem;"
					>
						<li>Model: claude-sonnet-4-5</li>
						<li>Tokens: 1,234 in / 567 out</li>
						<li>Duration: 2.3s</li>
					</ul>
					<button
						data-testid="detail-action"
						style="width: fit-content; padding: 0.375rem 0.75rem; font-size: 0.75rem; border: 1px solid var(--border-default); border-radius: 0.375rem; background: transparent; color: var(--text-1); cursor: pointer;"
					>
						View full request
					</button>
				</div>
			</Tabs.Content>
			<Tabs.Content value="raw" class="pt-4 text-[var(--text-2)]">
				<pre
					style="font-size: 0.75rem; overflow-x: auto; padding: 0.5rem; background: var(--surface-2, var(--bg-raised)); border-radius: 0.25rem;">{'{\n  "model": "claude-sonnet-4-5"\n}'}</pre>
			</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="CustomClass"
	tags={['design-exploration']}
	play={async ({ canvasElement }) => {
		const panel = canvasElement.querySelector('[data-slot="tabs-content"][data-state="active"]');
		await expect(panel, 'active panel must be present').not.toBeNull();
		await expect(panel).toHaveClass('pt-8');
		await expect(panel).toHaveClass('text-sm');
	}}
>
	{#snippet template()}
		<Tabs.Root value="custom" class="w-80">
			<Tabs.List>
				<Tabs.Trigger value="custom">Custom</Tabs.Trigger>
				<Tabs.Trigger value="normal">Normal</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="custom" class="pt-8 text-[var(--text-1)]">
				Custom padded content (pt-8 override)
			</Tabs.Content>
			<Tabs.Content value="normal" class="pt-4 text-[var(--text-1)]">Normal content</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="DataSlotAndRef"
	tags={['interactive', 'design-exploration']}
	play={async ({ canvasElement }) => {
		const activePanel = canvasElement.querySelector(
			'[data-slot="tabs-content"][data-state="active"]'
		);
		await expect(
			activePanel,
			'active panel with data-slot="tabs-content" must be in DOM'
		).not.toBeNull();
		const activePanels = canvasElement.querySelectorAll(
			'[data-slot="tabs-content"][data-state="active"]'
		);
		await expect(activePanels.length, 'only one content panel is active at a time').toBe(1);
		const inactivePanel = canvasElement.querySelector(
			'[data-slot="tabs-content"][data-state="inactive"]'
		);
		await expect(
			inactivePanel,
			'inactive panel must be in DOM with hidden attribute'
		).not.toBeNull();
		await expect(inactivePanel).toHaveAttribute('hidden');
	}}
>
	{#snippet template()}
		<Tabs.Root value="alpha" class="w-80">
			<Tabs.List>
				<Tabs.Trigger value="alpha">Alpha</Tabs.Trigger>
				<Tabs.Trigger value="beta">Beta</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="alpha" class="pt-4 text-[var(--text-1)]">Alpha panel</Tabs.Content>
			<Tabs.Content value="beta" class="pt-4 text-[var(--text-1)]">Beta panel</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="TabSwitching"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		tabSwitchingValueChangeSpy.mockClear();
		const canvas = within(canvasElement);

		let panel = canvasElement.querySelector('[data-slot="tabs-content"][data-state="active"]');
		await expect(panel?.textContent?.trim()).toContain('Messages panel content');

		const toolsTab = canvas.getByRole('tab', { name: 'Tools' });
		await userEvent.click(toolsTab);

		await expect(tabSwitchingValueChangeSpy).toHaveBeenCalledWith('tools');

		panel = canvasElement.querySelector('[data-slot="tabs-content"][data-state="active"]');
		await expect(panel?.textContent?.trim()).toContain('Tools panel content');

		const systemTab = canvas.getByRole('tab', { name: 'System' });
		await userEvent.click(systemTab);

		await expect(tabSwitchingValueChangeSpy).toHaveBeenCalledWith('system');

		panel = canvasElement.querySelector('[data-slot="tabs-content"][data-state="active"]');
		await expect(panel?.textContent?.trim()).toContain('System panel content');

		const activePanels = canvasElement.querySelectorAll(
			'[data-slot="tabs-content"][data-state="active"]'
		);
		await expect(activePanels.length).toBe(1);
	}}
>
	{#snippet template()}
		<Tabs.Root value="messages" class="w-80" onValueChange={tabSwitchingValueChangeSpy}>
			<Tabs.List>
				<Tabs.Trigger value="messages">Messages</Tabs.Trigger>
				<Tabs.Trigger value="tools">Tools</Tabs.Trigger>
				<Tabs.Trigger value="system">System</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="messages" class="pt-4 text-[var(--text-1)]">
				Messages panel content
			</Tabs.Content>
			<Tabs.Content value="tools" class="pt-4 text-[var(--text-1)]">
				Tools panel content
			</Tabs.Content>
			<Tabs.Content value="system" class="pt-4 text-[var(--text-1)]">
				System panel content
			</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story
	name="KeyboardWithinContent"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		keyboardWithinContentValueChangeSpy.mockClear();
		const canvas = within(canvasElement);

		const overviewTab = canvas.getByRole('tab', { name: 'Overview' });
		await userEvent.click(overviewTab);

		await expect(keyboardWithinContentValueChangeSpy).not.toHaveBeenCalled();

		await userEvent.keyboard('{Tab}{Tab}');
		const actionBtn = canvas.getByTestId('keyboard-action');
		await expect(document.activeElement).toBe(actionBtn);

		await expect(overviewTab).toHaveAttribute('data-state', 'active');

		await expect(keyboardWithinContentValueChangeSpy).not.toHaveBeenCalled();
	}}
>
	{#snippet template()}
		<Tabs.Root value="overview" class="w-80" onValueChange={keyboardWithinContentValueChangeSpy}>
			<Tabs.List>
				<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
				<Tabs.Trigger value="detail">Detail</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]">
				<button
					data-testid="keyboard-action"
					style="padding: 0.25rem 0.5rem; border: 1px solid var(--border-default); border-radius: 0.25rem; background: transparent; color: var(--text-1); cursor: pointer; font-size: 0.875rem;"
				>
					Focusable action
				</button>
			</Tabs.Content>
			<Tabs.Content value="detail" class="pt-4 text-[var(--text-1)]">Detail panel</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>

<Story name="AllThemes" tags={['design-exploration']}>
	{#snippet template()}
		<div style="display: flex; flex-direction: column; gap: 2rem; padding: 1rem;">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div
					data-theme={theme}
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
						</Tabs.List>
						<Tabs.Content value="active" class="pt-4 text-[var(--text-1)]">
							Active content on <strong>{theme}</strong> theme
						</Tabs.Content>
						<Tabs.Content value="inactive" class="pt-4 text-[var(--text-1)]">
							Inactive content
						</Tabs.Content>
					</Tabs.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotDefault"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-content/Default');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="overview" class="w-80">
				<Tabs.List>
					<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
					<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
					<Tabs.Trigger value="analysis">Analysis</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="overview" class="pt-4 text-[var(--text-1)]">
					Overview content panel
				</Tabs.Content>
				<Tabs.Content value="requests" class="pt-4 text-[var(--text-1)]">
					Requests content panel
				</Tabs.Content>
				<Tabs.Content value="analysis" class="pt-4 text-[var(--text-1)]">
					Analysis content panel
				</Tabs.Content>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotNestedHTML"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-content/NestedHTML');
	}}
>
	{#snippet template()}
		<div style="padding: 1rem;">
			<Tabs.Root value="detail" class="w-96">
				<Tabs.List>
					<Tabs.Trigger value="detail">Detail</Tabs.Trigger>
					<Tabs.Trigger value="raw">Raw</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="detail" class="pt-4 text-[var(--text-1)]">
					<div style="display: flex; flex-direction: column; gap: 0.75rem;">
						<h3 style="font-weight: 600; font-size: 0.875rem;">Request Detail</h3>
						<ul
							style="list-style: disc; padding-left: 1.25rem; font-size: 0.875rem; display: flex; flex-direction: column; gap: 0.25rem;"
						>
							<li>Model: claude-sonnet-4-5</li>
							<li>Tokens: 1,234 in / 567 out</li>
							<li>Duration: 2.3s</li>
						</ul>
					</div>
				</Tabs.Content>
				<Tabs.Content value="raw" class="pt-4 text-[var(--text-2)]">
					<pre style="font-size: 0.75rem;">{'{ "model": "claude-sonnet-4-5" }'}</pre>
				</Tabs.Content>
			</Tabs.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotAllThemes"
	tags={['visual', 'design-exploration']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tabs-content/AllThemes');
	}}
>
	{#snippet template()}
		<div style="display: flex; flex-direction: column; gap: 2rem; padding: 1rem;">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div
					data-theme={theme}
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
						</Tabs.List>
						<Tabs.Content value="active" class="pt-4 text-[var(--text-1)] text-sm">
							Active content on <strong>{theme}</strong> theme
						</Tabs.Content>
						<Tabs.Content value="inactive" class="pt-4 text-[var(--text-1)] text-sm">
							Inactive content
						</Tabs.Content>
					</Tabs.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="HiddenAttributeVariant"
	tags={['interactive', 'design-exploration']}
	play={async ({ canvasElement }) => {
		const allPanels = canvasElement.querySelectorAll('[data-slot="tabs-content"]');
		await expect(allPanels.length, 'all panels must be in DOM at all times').toBe(2);

		const activePanels = canvasElement.querySelectorAll(
			'[data-slot="tabs-content"][data-state="active"]'
		);
		await expect(activePanels.length, 'exactly one panel is active').toBe(1);
		await expect(activePanels[0]).not.toHaveAttribute('hidden');

		const inactivePanels = canvasElement.querySelectorAll(
			'[data-slot="tabs-content"][data-state="inactive"]'
		);
		await expect(inactivePanels.length, 'exactly one panel is inactive').toBe(1);
		await expect(inactivePanels[0]).toHaveAttribute('hidden');
	}}
>
	{#snippet template()}
		<Tabs.Root value="alpha" class="w-80">
			<Tabs.List>
				<Tabs.Trigger value="alpha">Alpha</Tabs.Trigger>
				<Tabs.Trigger value="beta">Beta</Tabs.Trigger>
			</Tabs.List>
			<Tabs.Content value="alpha" class="pt-4 text-[var(--text-1)]">
				Alpha panel — active, visible
			</Tabs.Content>
			<Tabs.Content value="beta" class="pt-4 text-[var(--text-1)]">
				Beta panel — inactive, hidden via HTML hidden attribute
			</Tabs.Content>
		</Tabs.Root>
	{/snippet}
</Story>
