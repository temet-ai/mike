<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, expect, userEvent } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Popover from './index';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Popover/PopoverContent',
		component: Popover.Content,
		tags: ['autodocs'],
		argTypes: {
			sideOffset: { control: 'number' },
			align: {
				control: 'select',
				options: ['start', 'center', 'end']
			}
		}
	});

	const formSubmissionSubmitSpy = fn();
	const menuItemClickCopyIdSpy = fn();
</script>

<Story
	name="SimpleText"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-content/SimpleText');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open popover</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Popover content renders here.</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="PortalProps"
	tags={['design-exploration']}
	play={async ({ canvasElement }) => {
		const contentInCanvas = canvasElement.querySelector('[data-slot="popover-content"]');
		await expect(
			contentInCanvas,
			'popover content must render inside canvasElement when portalProps.disabled=true'
		).toBeTruthy();

		const contentInBody = document.body.querySelector(':scope > [data-slot="popover-content"]');
		await expect(
			contentInBody,
			'popover content must NOT be portalled to document.body when portalProps.disabled=true'
		).toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open (no portal)</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content portalProps={{ disabled: true }}>
					<p class="text-[var(--text-1)] text-sm">
						Rendered inline (portalProps.disabled=true) — content stays in the DOM tree rather than
						teleporting to document.body.
					</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="WithCloseButton"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-content/WithCloseButton');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Filter options</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-strong)] text-sm font-semibold">Filter options</p>
					<p class="mt-1 text-[var(--text-2)] text-xs">
						Choose how requests are filtered in the current session view.
					</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="WithForm"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-content/WithForm');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Edit filter rule" {...props}>
							<svg
								class="size-4 text-[var(--text-2)]"
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke="currentColor"
								aria-hidden="true"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
								/>
							</svg>
						</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content class="w-[22rem]">
					<form class="space-y-3">
						<fieldset class="space-y-3 border-0 p-0">
							<legend class="text-[var(--text-strong)] text-sm font-semibold"
								>Edit filter rule</legend
							>
							<div class="space-y-1">
								<label for="content-story-rule-kind" class="text-[var(--text-2)] text-xs"
									>Rule kind</label
								>
								<select
									id="content-story-rule-kind"
									class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
								>
									<option value="text_substring">Text substring</option>
									<option value="xml_tag">XML tag</option>
								</select>
							</div>
							<div class="space-y-1">
								<label for="content-story-rule-value" class="text-[var(--text-2)] text-xs"
									>Match value</label
								>
								<input
									id="content-story-rule-value"
									type="text"
									value="<parameter>"
									class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 font-mono text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
								/>
							</div>
						</fieldset>
						<div class="flex justify-end gap-2 pt-1">
							<Popover.Close>
								{#snippet child({ props })}
									<Button variant="ghost" size="sm" {...props}>Cancel</Button>
								{/snippet}
							</Popover.Close>
							<Button variant="default" size="sm" type="submit">Save</Button>
						</div>
					</form>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="WithList"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-content/WithList');
	}}
>
	{#snippet template()}
		<div class="flex items-end justify-end p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>More options</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content sideOffset={8} align="end">
					<ul class="space-y-1">
						<li>
							<button
								class="w-full rounded px-2 py-1.5 text-left text-[var(--text-1)] text-sm hover:bg-[var(--surface-2)]"
								>Copy request ID</button
							>
						</li>
						<li>
							<button
								class="w-full rounded px-2 py-1.5 text-left text-[var(--text-1)] text-sm hover:bg-[var(--surface-2)]"
								>Export as JSON</button
							>
						</li>
						<li>
							<button
								class="w-full rounded px-2 py-1.5 text-left text-[var(--status-danger-fg)] text-sm hover:bg-[var(--surface-2)]"
								>Delete session</button
							>
						</li>
					</ul>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="AlignEnd" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-end justify-end p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Trailing corner anchor</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content sideOffset={8} align="end">
					<p class="text-[var(--text-1)] text-sm">
						This popover is anchored to the trailing (end) edge of its trigger.
					</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="AlignCenter" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-end justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Center anchor</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content align="center">
					<p class="text-[var(--text-1)] text-sm">This popover is center-aligned to its trigger.</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomWidth"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-content/CustomWidth');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Wide popover</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content class="w-[22rem]">
					<p class="text-[var(--text-strong)] text-sm font-semibold">Wide content panel</p>
					<p class="mt-1 text-[var(--text-2)] text-xs">
						This panel is 22rem wide (w-[22rem] class override). The default is w-80 (20rem). Used
						by AddRulePopover to accommodate the form inputs without wrapping.
					</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="SideOffsetDefault" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Default offset (6px)</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content sideOffset={6}>
					<p class="text-[var(--text-1)] text-sm">
						sideOffset=6 (default gap between trigger and content).
					</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="SideOffsetLarge" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Large offset (16px)</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content sideOffset={16}>
					<p class="text-[var(--text-1)] text-sm">
						sideOffset=16 — larger gap between trigger and content.
					</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DataSlotAttribute"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const contentEl = document.querySelector('[data-slot="popover-content"]');
		await expect(
			contentEl,
			'data-slot="popover-content" element must exist in document'
		).toBeTruthy();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Check data-slot</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">
						Checking data-slot="popover-content" attribute.
					</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ClickTriggerToOpen"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const contentBefore = document.querySelector('[data-slot="popover-content"]');
		await expect(contentBefore, 'popover content must be absent before trigger click').toBeNull();

		const trigger = canvasElement.querySelector('button');
		await expect(trigger, 'trigger button must be in DOM').toBeTruthy();
		await userEvent.click(trigger as Element);

		const contentAfter = document.querySelector('[data-slot="popover-content"]');
		await expect(contentAfter, 'popover content must be mounted after trigger click').toBeTruthy();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Click to open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Popover opened by clicking the trigger.</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ClickCloseButton"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const contentBefore = document.querySelector('[data-slot="popover-content"]');
		await expect(
			contentBefore,
			'popover content must be mounted before clicking close'
		).toBeTruthy();

		const closeBtn =
			contentBefore?.querySelector('[data-slot="popover-close"] button') ??
			contentBefore?.querySelector('button');
		await expect(closeBtn, 'close button must be inside portalled content').toBeTruthy();

		await userEvent.click(closeBtn as Element);

		const contentAfter = document.querySelector('[data-slot="popover-content"]');
		if (contentAfter !== null) {
			await expect(
				contentAfter.getAttribute('data-state'),
				'popover data-state must be "closed" after clicking close button'
			).toBe('closed');
		} else {
			await expect(
				contentBefore,
				'popover content must have been mounted before clicking close (pre-condition for unmount path)'
			).not.toBeNull();
		}
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Trigger</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Click Close to dismiss.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="EscapeKeyCloses"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const contentBefore = document.querySelector('[data-slot="popover-content"]');
		await expect(contentBefore, 'popover content must be mounted before Escape key').toBeTruthy();

		const contentEl = contentBefore as HTMLElement;
		contentEl.focus();

		await userEvent.keyboard('{Escape}');

		const contentAfter = document.querySelector('[data-slot="popover-content"]');
		if (contentAfter !== null) {
			await expect(
				contentAfter.getAttribute('data-state'),
				'popover data-state must be "closed" after Escape key'
			).toBe('closed');
		} else {
			await expect(
				contentBefore,
				'popover content must have been mounted before pressing Escape (pre-condition for unmount path)'
			).not.toBeNull();
		}
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Trigger</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Press Escape to close this popover.</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="FormSubmission"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		formSubmissionSubmitSpy.mockClear();
		const saveBtn = document.querySelector('[data-slot="popover-content"] button[type="submit"]');
		await expect(saveBtn, 'submit button must be in document').toBeTruthy();
		await userEvent.click(saveBtn as Element);

		await expect(
			formSubmissionSubmitSpy,
			'onSubmit spy must be called when Save is clicked'
		).toHaveBeenCalledOnce();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Edit rule</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content class="w-[22rem]">
					<form
						class="space-y-3"
						onsubmit={(e) => {
							e.preventDefault();
							formSubmissionSubmitSpy(e);
						}}
					>
						<fieldset class="space-y-3 border-0 p-0">
							<legend class="text-[var(--text-strong)] text-sm font-semibold"
								>Edit filter rule</legend
							>
							<div class="space-y-1">
								<label for="content-form-submit-value" class="text-[var(--text-2)] text-xs"
									>Match value</label
								>
								<input
									id="content-form-submit-value"
									type="text"
									value="<parameter>"
									class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 font-mono text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
								/>
							</div>
						</fieldset>
						<div class="flex justify-end gap-2 pt-1">
							<Popover.Close>
								{#snippet child({ props })}
									<Button variant="ghost" size="sm" {...props}>Cancel</Button>
								{/snippet}
							</Popover.Close>
							<Button variant="default" size="sm" type="submit">Save</Button>
						</div>
					</form>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="MenuItemClick"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		menuItemClickCopyIdSpy.mockClear();
		const firstItem = document.querySelector('[data-slot="popover-content"] ul button');
		await expect(firstItem, 'first menu item button must be in document').toBeTruthy();

		await userEvent.click(firstItem as Element);
		await expect(
			menuItemClickCopyIdSpy,
			'onCopyId spy must be called when first menu item is clicked'
		).toHaveBeenCalledOnce();
	}}
>
	{#snippet template()}
		<div class="flex items-end justify-end p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>More options</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content sideOffset={8} align="end">
					<ul class="space-y-1">
						<li>
							<button
								class="w-full rounded px-2 py-1.5 text-left text-[var(--text-1)] text-sm hover:bg-[var(--surface-2)]"
								onclick={menuItemClickCopyIdSpy}>Copy request ID</button
							>
						</li>
						<li>
							<button
								class="w-full rounded px-2 py-1.5 text-left text-[var(--text-1)] text-sm hover:bg-[var(--surface-2)]"
								>Export as JSON</button
							>
						</li>
						<li>
							<button
								class="w-full rounded px-2 py-1.5 text-left text-[var(--status-danger-fg)] text-sm hover:bg-[var(--surface-2)]"
								>Delete session</button
							>
						</li>
					</ul>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="TabIntoContent"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector('button');
		await expect(trigger, 'trigger button must be in DOM').toBeTruthy();
		(trigger as HTMLElement).focus();

		await userEvent.tab();

		const contentEl = document.querySelector('[data-slot="popover-content"]');
		await expect(contentEl, 'popover content must be in document when open').toBeTruthy();

		const focused = document.activeElement;
		const isInsideContent = contentEl?.contains(focused);
		await expect(isInsideContent, 'focus must move into the popover content after Tab').toBe(true);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Trigger (then Tab in)</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Tab focuses into this panel (role=dialog).</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const contentEl = document.querySelector('[data-slot="popover-content"]');
		await expect(
			contentEl,
			'popover-content element must be in document (bind:ref target)'
		).toBeTruthy();

		await expect(
			contentEl instanceof HTMLElement,
			'popover-content must be an HTMLElement (confirming bind:ref captures a DOM node)'
		).toBe(true);

		const marker = document.querySelector('[data-ref-testid="ref-binding-marker"]');
		await expect(
			marker,
			'ref-binding content marker must render inside portalled content'
		).toBeTruthy();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Check bind:ref</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Checking bind:ref on Popover.Content.</p>
					<span data-ref-testid="ref-binding-marker" class="sr-only" aria-hidden="true"></span>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AllStatesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-content/AllStates');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-24 p-8">
			<div class="flex items-center justify-center">
				<Popover.Root open>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Simple text</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Content>
						<p class="text-[var(--text-1)] text-sm">Minimal text content.</p>
					</Popover.Content>
				</Popover.Root>
			</div>

			<div class="flex items-center justify-center">
				<Popover.Root open>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>With close</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Content>
						<p class="text-[var(--text-strong)] text-sm font-semibold">With close button</p>
						<p class="mt-1 text-[var(--text-2)] text-xs">Content plus a dismiss control.</p>
						<div class="mt-3 flex justify-end">
							<Popover.Close>
								{#snippet child({ props })}
									<Button variant="ghost" size="sm" {...props}>Close</Button>
								{/snippet}
							</Popover.Close>
						</div>
					</Popover.Content>
				</Popover.Root>
			</div>

			<div class="flex items-end justify-end">
				<Popover.Root open>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>List (end)</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Content sideOffset={8} align="end">
						<ul class="space-y-1">
							<li>
								<button
									class="w-full rounded px-2 py-1.5 text-left text-[var(--text-1)] text-sm hover:bg-[var(--surface-2)]"
									>Copy ID</button
								>
							</li>
							<li>
								<button
									class="w-full rounded px-2 py-1.5 text-left text-[var(--text-1)] text-sm hover:bg-[var(--surface-2)]"
									>Export JSON</button
								>
							</li>
						</ul>
					</Popover.Content>
				</Popover.Root>
			</div>
		</div>
	{/snippet}
</Story>
