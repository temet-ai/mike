<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within, waitFor } from 'storybook/test';
	import * as Dialog from './index';
	import { Button } from '$lib/components/ui/button';
	import DialogContent from './dialog-content.svelte';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';

	const { Story } = defineMeta({
		title: 'UI/Dialog/DialogContent',
		component: DialogContent,
		tags: ['autodocs'],
		argTypes: {
			showCloseButton: { control: 'boolean' }
		}
	});

	let controlledOpen = $state(true);
	let boundRef = $state<HTMLElement | null>(null);
	const controlledOpenSpy = fn();
	const clickCloseButtonDismissesSpy = fn();
	const escKeyDismissesSpy = fn();
</script>

<Story
	name="Default"
	tags={['visual']}
	play={async () => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-content/Default', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Confirm action</Dialog.Title>
					<Dialog.Description>
						This is the default DialogContent with the full chrome: header, title, description, and
						footer with action buttons. The close button (X) is rendered top-right by default.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">Confirm</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="WithoutCloseButton"
	tags={['interactive']}
	play={async () => {
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(content, 'dialog-content must be present in DOM').toBeTruthy();
		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBtn, 'close button must be absent when showCloseButton=false').toBeNull();
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false}>
				<Dialog.Header>
					<Dialog.Title>No close button</Dialog.Title>
					<Dialog.Description>
						showCloseButton=false — the XIcon close button slot is omitted. Dismiss via ESC or
						clicking the overlay backdrop.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">OK</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ControlledOpen"
	tags={['interactive']}
	play={async () => {
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(content, 'dialog content must be in DOM (starts open)').toBeTruthy();
		await expect(
			content?.getAttribute('data-state'),
			'dialog must be data-state="open" at start'
		).toBe('open');

		const closeBtn = document.body.querySelector(
			'[data-slot="dialog-close"]'
		) as HTMLElement | null;
		await expect(closeBtn, 'close button must be present').toBeTruthy();
		await userEvent.click(closeBtn!);

		await expect(controlledOpenSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Button variant="outline" onclick={() => (controlledOpen = !controlledOpen)}>
				Toggle dialog (state: {controlledOpen ? 'open' : 'closed'})
			</Button>
			<Dialog.Root bind:open={controlledOpen} onOpenChange={controlledOpenSpy}>
				<Dialog.Content>
					<Dialog.Header>
						<Dialog.Title>Controlled open state</Dialog.Title>
						<Dialog.Description>
							This dialog's open state is two-way bound to a parent-owned reactive boolean via
							bind:open — the production usage shape. Closing writes false back.
						</Dialog.Description>
					</Dialog.Header>
					<Dialog.Footer>
						<Dialog.Close>
							{#snippet child({ props })}
								<Button variant="outline" {...props}>Close</Button>
							{/snippet}
						</Dialog.Close>
					</Dialog.Footer>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story name="CustomClass" tags={['design-exploration']}>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="max-w-md">
				<Dialog.Header>
					<Dialog.Title>Custom class: max-w-md</Dialog.Title>
					<Dialog.Description>
						This dialog uses class="max-w-md" which overrides the default max-w-lg via cn() +
						tailwind-merge. The dialog should be narrower than the default.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="EmptyClass" tags={['design-exploration']}>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="">
				<Dialog.Header>
					<Dialog.Title>Empty class string</Dialog.Title>
					<Dialog.Description>
						class="" — empty string passed to class prop. cn() deduplication must not produce
						invalid output. Dialog renders with default classes only.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="LongContent"
	tags={['interactive']}
	play={async () => {
		const dialog = await within(document.body).findByRole('dialog', {
			name: 'Session bundle export'
		});
		await expect(getComputedStyle(dialog).getPropertyValue('--aug-border-all').trim()).toBe('1px');
		await expect(getComputedStyle(dialog).animationName).toBe('funk-dialog-in');
		await waitFor(() => {
			const bounds = dialog.getBoundingClientRect();
			expect(bounds.top).toBeGreaterThanOrEqual(0);
			expect(bounds.bottom).toBeLessThanOrEqual(window.innerHeight);
			const body = dialog.querySelector('[data-slot="dialog-body"]') as HTMLElement;
			expect(body.scrollHeight).toBeGreaterThan(body.clientHeight);
		});
		await expect(dialog.scrollHeight).toBeLessThanOrEqual(dialog.clientHeight);
		const body = dialog.querySelector('[data-slot="dialog-body"]') as HTMLElement;
		body.scrollTop = body.scrollHeight;
		await expect(body.scrollTop).toBeGreaterThan(0);
		await expect(dialog.scrollTop).toBe(0);
		const exportButton = within(dialog).getByRole('button', { name: 'Export' });
		await expect(exportButton.getBoundingClientRect().bottom).toBeLessThanOrEqual(
			window.innerHeight
		);
		await expect(within(dialog).getByText('Captured request 40')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Session bundle export</Dialog.Title>
					<Dialog.Description>
						This dialog has a longer description to verify that the grid gap-4 layout (16px between
						children slots) renders correctly. The header, body content, and footer each occupy
						their own grid row with equal gap spacing between them.
					</Dialog.Description>
				</Dialog.Header>
				<p class="text-muted-foreground text-sm">
					Additional body content rendered as a grid child between the header and footer. This slot
					demonstrates the gap-4 grid layout in a realistic three-row dialog shape.
				</p>
				<div class="space-y-3">
					{#each Array.from({ length: 40 }, (_, index) => index + 1) as request (request)}
						<p>Captured request {request}</p>
					{/each}
				</div>
				<Dialog.Footer>
					<Button variant="ghost">Skip</Button>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">Export</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="RestPropsPassthrough"
	tags={['interactive']}
	play={async () => {
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(content, 'dialog-content must be present in DOM').toBeTruthy();
		await expect(
			content?.getAttribute('data-testid'),
			'data-testid restProp must be forwarded to dialog-content'
		).toBe('my-dialog-content');
		await expect(
			content?.getAttribute('aria-label'),
			'aria-label restProp must be forwarded to dialog-content'
		).toBe('Custom aria label');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content data-testid="my-dialog-content" aria-label="Custom aria label">
				<Dialog.Header>
					<Dialog.Title>Rest props passthrough</Dialog.Title>
					<Dialog.Description>
						data-testid and aria-label are passed via ...restProps to DialogPrimitive.Content.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="BindRef"
	tags={['interactive']}
	play={async () => {
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

		await expect(
			boundRef,
			'bind:ref must resolve to an HTMLElement after mount (not null)'
		).toBeInstanceOf(HTMLElement);

		await expect(
			boundRef?.getAttribute('data-slot'),
			'bound ref must point to the element with data-slot="dialog-content"'
		).toBe('dialog-content');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content bind:ref={boundRef}>
				<Dialog.Header>
					<Dialog.Title>bind:ref exercise</Dialog.Title>
					<Dialog.Description>
						The ref prop ($bindable) must resolve to the underlying HTMLElement rendered by
						DialogPrimitive.Content after the component mounts. The play function asserts the
						module-level boundRef variable is an HTMLElement, not a portal DOM check.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="PortalProps" tags={['design-exploration']}>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content portalProps={{}}>
				<Dialog.Header>
					<Dialog.Title>Portal props passthrough</Dialog.Title>
					<Dialog.Description>
						portalProps (empty object) is forwarded to DialogPortal via spread. This story confirms
						the prop is accepted and the dialog still renders correctly.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="AllThemes" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex flex-wrap gap-6 p-4">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme (theme)}
				<div data-theme={theme} class="rounded-lg bg-background p-2">
					<p class="text-foreground mb-2 text-xs font-medium">{theme}</p>
					<Dialog.Root open>
						<Dialog.Content class="static translate-x-0 translate-y-0 max-w-xs">
							<Dialog.Header>
								<Dialog.Title>Dialog ({theme})</Dialog.Title>
								<Dialog.Description>
									Theme-aware bg-card, text-card-foreground, border-border, rounded-[--radius].
								</Dialog.Description>
							</Dialog.Header>
							<Dialog.Footer>
								<Button variant="outline" class="text-xs">Cancel</Button>
								<Button variant="default" class="text-xs">Confirm</Button>
							</Dialog.Footer>
						</Dialog.Content>
					</Dialog.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async () => {
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

		const themes = ['forge', 'glacier', 'void', 'phosphor', 'crimson'];
		const allContents = document.body.querySelectorAll('[data-slot="dialog-content"]');

		for (let i = 0; i < themes.length; i++) {
			const theme = themes[i];
			const dialogEl = allContents[i] as HTMLElement | undefined;
			if (dialogEl) {
				await captureStorySnapshot(document.body as HTMLElement, `dialog-content/theme-${theme}`, {
					selector: `[data-slot="dialog-content"]:nth-of-type(${i + 1})`
				});
			}
		}
	}}
>
	{#snippet template()}
		<div class="flex flex-wrap gap-6 p-4">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme (theme)}
				<div data-theme={theme} class="rounded-lg bg-background p-2">
					<p class="text-foreground mb-2 text-xs font-medium">{theme}</p>
					<Dialog.Root open>
						<Dialog.Content class="static translate-x-0 translate-y-0 max-w-xs">
							<Dialog.Header>
								<Dialog.Title>Dialog ({theme})</Dialog.Title>
								<Dialog.Description>Theme bg/text/border/radius.</Dialog.Description>
							</Dialog.Header>
							<Dialog.Footer>
								<Button variant="outline" class="text-xs">Cancel</Button>
								<Button variant="default" class="text-xs">OK</Button>
							</Dialog.Footer>
						</Dialog.Content>
					</Dialog.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="DataSlotAttribute"
	tags={['interactive']}
	play={async () => {
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(content, 'dialog-content element must be in DOM').toBeTruthy();
		await expect(
			content?.getAttribute('data-slot'),
			'element must have data-slot="dialog-content"'
		).toBe('dialog-content');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Data slot check</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ShowCloseButtonTrue"
	tags={['interactive']}
	play={async () => {
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(content, 'dialog-content must be present in DOM').toBeTruthy();
		const closeBtn = document.body.querySelector(
			'[data-slot="dialog-close"]'
		) as HTMLElement | null;
		await expect(closeBtn, 'close button must be present when showCloseButton=true').toBeTruthy();
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={true}>
				<Dialog.Header>
					<Dialog.Title>Close button present</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ShowCloseButtonUndefined"
	tags={['interactive']}
	play={async () => {
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(content, 'dialog-content must be present in DOM').toBeTruthy();
		const closeBtn = document.body.querySelector(
			'[data-slot="dialog-close"]'
		) as HTMLElement | null;
		await expect(
			closeBtn,
			'close button must be present when showCloseButton is omitted (default=true)'
		).toBeTruthy();
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Default showCloseButton</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ChildrenSnippetRender"
	tags={['interactive']}
	play={async () => {
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(content, 'dialog-content must be present in DOM').toBeTruthy();
		const customChild = content?.querySelector(
			'[data-testid="custom-child"]'
		) as HTMLElement | null;
		await expect(
			customChild,
			'children snippet must render its content inside dialog-content'
		).toBeTruthy();
		await expect(
			customChild?.textContent?.trim(),
			'custom child must have the expected text content'
		).toBe('Custom child rendered');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Children snippet render</Dialog.Title>
				</Dialog.Header>
				<p data-testid="custom-child" class="text-sm">Custom child rendered</p>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="KeyboardTabFocusTrap"
	tags={['interactive']}
	play={async () => {
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(content, 'dialog content must be in DOM').toBeTruthy();

		const closeBtn = document.body.querySelector(
			'[data-slot="dialog-close"]'
		) as HTMLElement | null;
		await expect(closeBtn, 'close button must be in DOM to seed focus trap').toBeTruthy();
		closeBtn!.focus();

		await userEvent.tab();

		const focused = document.activeElement as HTMLElement | null;
		await expect(
			content!.contains(focused),
			`focus after Tab must remain inside dialog-content, but landed on ${focused?.tagName ?? 'null'}`
		).toBe(true);
		await expect(
			focused?.tagName.toLowerCase() === 'button' || focused?.closest('button') !== null,
			'focused element after Tab must be a button (or inside a button) within the dialog'
		).toBe(true);

		await userEvent.tab({ shift: true });
		const focusedAfterShiftTab = document.activeElement as HTMLElement | null;
		await expect(
			content!.contains(focusedAfterShiftTab),
			`focus after Shift+Tab must remain inside dialog-content, but landed on ${focusedAfterShiftTab?.tagName ?? 'null'}`
		).toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Focus trap verification</Dialog.Title>
					<Dialog.Description>
						Tab and Shift+Tab should cycle focus only within the dialog. bits-ui manages the focus
						trap automatically on DialogPrimitive.Content.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">Confirm</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="AccessibilityRoleAndModal" tags={['design-exploration']}>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Accessibility attributes</Dialog.Title>
					<Dialog.Description>
						bits-ui sets role=dialog and aria-modal=true on DialogPrimitive.Content.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="AccessibilityLabelledBy" tags={['design-exploration']}>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Labelled dialog</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ClickCloseButtonDismisses"
	tags={['interactive']}
	play={async () => {
		clickCloseButtonDismissesSpy.mockClear();
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

		const contentBefore = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(contentBefore, 'dialog content must be in DOM before close click').toBeTruthy();
		await expect(
			contentBefore?.getAttribute('data-state'),
			'dialog must be data-state="open" before close click'
		).toBe('open');

		const closeBtn = document.body.querySelector(
			'[data-slot="dialog-close"]'
		) as HTMLElement | null;
		await expect(closeBtn, 'close button must be in DOM').toBeTruthy();
		await userEvent.click(closeBtn!);

		await expect(clickCloseButtonDismissesSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<Dialog.Root open onOpenChange={clickCloseButtonDismissesSpy}>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Click close button</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="EscKeyDismisses"
	tags={['interactive']}
	play={async () => {
		escKeyDismissesSpy.mockClear();
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

		const contentBefore = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		await expect(contentBefore, 'dialog content must be in DOM before ESC').toBeTruthy();
		await expect(
			contentBefore?.getAttribute('data-state'),
			'dialog must be data-state="open" before ESC'
		).toBe('open');

		const closeBtn = document.body.querySelector(
			'[data-slot="dialog-close"]'
		) as HTMLElement | null;
		if (closeBtn) {
			closeBtn.focus();
		}

		await userEvent.keyboard('{Escape}');

		await expect(escKeyDismissesSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<Dialog.Root open onOpenChange={escKeyDismissesSpy}>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>ESC dismisses dialog</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="WithoutCloseButtonSnapshot"
	tags={['visual']}
	play={async () => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-content/WithoutCloseButton', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false}>
				<Dialog.Header>
					<Dialog.Title>No close button</Dialog.Title>
					<Dialog.Description>
						showCloseButton=false: the XIcon close button slot is omitted from the render tree.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">OK</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="LongContentSnapshot"
	tags={['visual']}
	play={async () => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-content/LongContent', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Long content layout</Dialog.Title>
					<Dialog.Description>
						This story verifies the grid gap-4 layout (16px spacing) between the Header slot, an
						additional body paragraph, and the Footer slot.
					</Dialog.Description>
				</Dialog.Header>
				<p class="text-muted-foreground text-sm">
					Additional body content occupying a grid row between header and footer.
				</p>
				<Dialog.Footer>
					<Button variant="ghost">Skip</Button>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">Export</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>
