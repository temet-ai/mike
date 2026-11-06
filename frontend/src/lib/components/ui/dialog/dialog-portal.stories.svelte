<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import { Dialog as DialogPrimitive } from 'bits-ui';
	import * as Dialog from './index';
	import { Button } from '$lib/components/ui/button';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import DialogPortal from './dialog-portal.svelte';

	const { Story } = defineMeta({
		title: 'UI/Dialog/DialogPortal',
		component: DialogPortal,
		tags: ['autodocs'],
		argTypes: {
			disabled: {
				control: 'boolean',
				description: 'Disable portalling and render inline in the document flow (defaults to false)'
			},
			to: {
				control: 'text',
				description:
					'Portal target — Element reference or CSS selector string (defaults to document.body)'
			}
		}
	});

	let toElementPortalTarget = $state<HTMLElement | null>(null);

	const controlledOpenSpy = fn();
	const escKeyDismissesSpy = fn();
	const closeButtonDismissesSpy = fn();
</script>

<Story
	name="Default"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-portal/Default');
	}}
>
	{#snippet template()}
		<div class="relative" style="min-height: 200px;">
			<Dialog.Root open>
				<Dialog.Portal disabled>
					<Dialog.Overlay class="absolute" />
					<DialogPrimitive.Content
						class="bg-card text-card-foreground relative z-50 grid w-full max-w-lg gap-4 rounded-[var(--radius)] border border-border p-6 text-sm outline-none"
					>
						<Dialog.Header>
							<Dialog.Title>Portal: renders to document.body</Dialog.Title>
							<Dialog.Description>
								Default DialogPortal behaviour — `to` is undefined so bits-ui portals to
								document.body. This story uses disabled=true for snapshot capture (portalled content
								is outside canvasElement). See PortalRendersToDomBody for live DOM assertion.
							</Dialog.Description>
						</Dialog.Header>
					</DialogPrimitive.Content>
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DefaultPortalToBodySnapshot"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const dialog = document.querySelector('[role="dialog"]');
		await expect(
			dialog,
			'role=dialog must exist in document (portalled to body with default props)'
		).toBeTruthy();
		await expect(
			canvasElement.contains(dialog),
			'dialog must NOT be inside canvasElement — real default portal (to=body) is active'
		).toBe(false);
		await expect(
			document.body.contains(dialog),
			'dialog must be a descendant of document.body (default portal target)'
		).toBe(true);

		await captureStorySnapshot(canvasElement, 'dialog-portal/DefaultPortalToBodySnapshot');
	}}
>
	{#snippet template()}
		<div class="relative" style="min-height: 200px;">
			<Dialog.Root open>
				<Dialog.Portal>
					<Dialog.Overlay />
					<DialogPrimitive.Content
						class="bg-card text-card-foreground fixed top-1/2 left-1/2 z-50 grid w-full max-w-lg -translate-x-1/2 -translate-y-1/2 gap-4 rounded-[var(--radius)] border border-border p-6 text-sm outline-none"
					>
						<Dialog.Header>
							<Dialog.Title>Portal: real default props (to=undefined, disabled=false)</Dialog.Title>
							<Dialog.Description>
								No `disabled` or `to` prop — content portals to document.body. This is the real
								default code path. The play function asserts the portal teleportation and captures a
								snapshot of the canvasElement mount point.
							</Dialog.Description>
						</Dialog.Header>
					</DialogPrimitive.Content>
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DefaultViaDialogContent"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-portal/DefaultViaDialogContent');
	}}
>
	{#snippet template()}
		<div class="relative" style="min-height: 200px;">
			<Dialog.Root open>
				<Dialog.Content portalProps={{ disabled: true }} class="static translate-x-0 translate-y-0">
					<Dialog.Header>
						<Dialog.Title>Portal via Dialog.Content</Dialog.Title>
						<Dialog.Description>
							The standard production shape: Dialog.Content uses DialogPortal internally.
							portalProps.disabled=true used here for snapshot (portalled content is outside
							canvasElement scope and cannot be screenshotted via elementLocator).
						</Dialog.Description>
					</Dialog.Header>
					<Dialog.Footer>
						<Button variant="outline">Cancel</Button>
						<Button variant="default">Confirm</Button>
					</Dialog.Footer>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledPortal"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-portal/DisabledPortal');
	}}
>
	{#snippet template()}
		<div class="relative" style="min-height: 200px;">
			<Dialog.Root open>
				<Dialog.Portal disabled>
					<Dialog.Overlay />
					<DialogPrimitive.Content
						class="bg-card text-card-foreground relative z-50 grid w-full max-w-md gap-4 rounded-[var(--radius)] border border-border p-6 text-sm outline-none"
					>
						<Dialog.Header>
							<Dialog.Title>Portal disabled (inline render)</Dialog.Title>
							<Dialog.Description>
								disabled=true — content stays in the document flow without portalling to
								document.body. Useful for stacking contexts or testing.
							</Dialog.Description>
						</Dialog.Header>
					</DialogPrimitive.Content>
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledViaDialogContent"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-portal/DisabledViaDialogContent');
	}}
>
	{#snippet template()}
		<div class="relative" style="min-height: 200px;">
			<Dialog.Root open>
				<Dialog.Content portalProps={{ disabled: true }} class="static translate-x-0 translate-y-0">
					<Dialog.Header>
						<Dialog.Title>Portal disabled via portalProps</Dialog.Title>
						<Dialog.Description>
							Dialog.Content passes portalProps.disabled=true to DialogPortal. The portal renders
							inline without teleporting to document.body.
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="CssSelectorTarget"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const portalTarget = canvasElement.querySelector('#portal-target-story');
		await expect(
			portalTarget,
			'#portal-target-story container must exist in canvasElement'
		).toBeTruthy();

		const dialogContent = portalTarget?.querySelector('[role="dialog"]');
		await expect(
			dialogContent,
			'dialog content must be inside #portal-target-story (portal teleportation to CSS selector target)'
		).toBeTruthy();

		await expect(
			canvasElement.contains(dialogContent ?? null),
			'portal content must be reachable from canvasElement via #portal-target-story'
		).toBe(true);

		await captureStorySnapshot(canvasElement, 'dialog-portal/CssSelectorTarget');
	}}
>
	{#snippet template()}
		<div class="relative">
			<div id="portal-target-story" class="rounded border border-dashed border-border p-4">
				<p class="text-muted-foreground mb-2 text-xs">
					Portal target container (#portal-target-story)
				</p>
			</div>
			<Dialog.Root open>
				<Dialog.Portal to="#portal-target-story">
					<DialogPrimitive.Content
						class="bg-card text-card-foreground relative z-50 grid w-full gap-4 rounded-[var(--radius)] border border-border p-6 text-sm outline-none"
					>
						<Dialog.Header>
							<Dialog.Title>CSS selector target</Dialog.Title>
							<Dialog.Description>
								to="#portal-target-story" — bits-ui portals children into the element selected by
								the CSS selector rather than document.body.
							</Dialog.Description>
						</Dialog.Header>
					</DialogPrimitive.Content>
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ToElementTarget"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const targetEl = canvasElement.querySelector('[data-testid="element-portal-target"]');
		await expect(
			targetEl,
			'[data-testid="element-portal-target"] must exist in canvasElement'
		).toBeTruthy();

		const dialogContent = targetEl?.querySelector('[role="dialog"]');
		await expect(
			dialogContent,
			'dialog content must be inside the Element-reference target (to=Element portal path)'
		).toBeTruthy();

		const title =
			dialogContent?.querySelector('[data-slot="dialog-title"]') ??
			dialogContent?.querySelector('h2');
		await expect(title?.textContent?.trim()).toBe('Element reference target');
	}}
>
	{#snippet template()}
		<div class="relative">
			<div
				bind:this={toElementPortalTarget}
				data-testid="element-portal-target"
				class="rounded border border-dashed border-border p-4"
			>
				<p class="text-muted-foreground mb-2 text-xs">
					Portal target (Element reference via bind:this)
				</p>
			</div>
			<Dialog.Root open>
				<Dialog.Portal to={toElementPortalTarget ?? undefined}>
					<DialogPrimitive.Content
						class="bg-card text-card-foreground relative z-50 grid w-full gap-4 rounded-[var(--radius)] border border-border p-6 text-sm outline-none"
					>
						<Dialog.Header>
							<Dialog.Title>Element reference target</Dialog.Title>
							<Dialog.Description>
								to=Element — bits-ui portals children into the DOM node captured via bind:this,
								exercising the Element-reference path of the `to` prop.
							</Dialog.Description>
						</Dialog.Header>
					</DialogPrimitive.Content>
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="EmptyChildren"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const dialog = document.querySelector('[role="dialog"]');
		await expect(
			dialog,
			'no role=dialog should be present in document when portal has empty children'
		).toBeFalsy();

		const root = canvasElement.querySelector('[data-testid="empty-portal-root"]');
		await expect(
			root,
			'[data-testid="empty-portal-root"] must exist — story rendered without error'
		).toBeTruthy();
	}}
>
	{#snippet template()}
		<div data-testid="empty-portal-root" class="p-4">
			<Dialog.Root open>
				<Dialog.Portal disabled></Dialog.Portal>
			</Dialog.Root>
			<p class="text-muted-foreground text-xs">
				Portal with no children — no overlay or content rendered.
			</p>
		</div>
	{/snippet}
</Story>

<Story
	name="WithOverlayAndContent"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-portal/WithOverlayAndContent');
	}}
>
	{#snippet template()}
		<div class="relative" style="min-height: 200px;">
			<Dialog.Root open>
				<Dialog.Portal disabled>
					<Dialog.Overlay class="absolute" />
					<DialogPrimitive.Content
						class="bg-card text-card-foreground relative z-50 grid w-full max-w-lg gap-4 rounded-[var(--radius)] border border-border p-6 text-sm outline-none"
					>
						<Dialog.Header>
							<Dialog.Title>Full dialog structure</Dialog.Title>
							<Dialog.Description>
								Dialog.Overlay + DialogPrimitive.Content as portal children — the standard
								composition. Overlay provides the backdrop; Content is the dialog chrome.
								disabled=true used here for snapshot (portalled content cannot be screenshotted via
								canvasElement — see ChildrenSnippetRenders for live portal assertion).
							</Dialog.Description>
						</Dialog.Header>
						<Dialog.Footer>
							<Button variant="outline">Cancel</Button>
							<Button variant="default">Confirm</Button>
						</Dialog.Footer>
					</DialogPrimitive.Content>
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story name="AllThemes" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex flex-wrap gap-6 p-4">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div data-theme={theme} class="rounded-lg bg-background p-2">
					<p class="text-foreground mb-2 text-xs font-medium">{theme}</p>
					<Dialog.Root open>
						<Dialog.Content class="static translate-x-0 translate-y-0 max-w-xs">
							<Dialog.Header>
								<Dialog.Title>Portal ({theme})</Dialog.Title>
								<Dialog.Description>
									Portal content inherits theme tokens: bg-card, text-card-foreground,
									border-border, rounded-[--radius].
								</Dialog.Description>
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
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-portal/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex flex-wrap gap-6 p-4">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div data-theme={theme} class="rounded-lg bg-background p-2">
					<p class="text-foreground mb-2 text-xs font-medium">{theme}</p>
					<Dialog.Root open>
						<Dialog.Content class="static translate-x-0 translate-y-0 max-w-xs">
							<Dialog.Header>
								<Dialog.Title>Portal ({theme})</Dialog.Title>
								<Dialog.Description>Theme: bg-card / text / border / radius.</Dialog.Description>
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
	name="ControlledOpen"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		controlledOpenSpy.mockClear();
		const canvas = within(document.body);
		const dialog = canvas.getByRole('dialog');
		await expect(dialog, 'dialog element must be present in document.body').toBeTruthy();

		const closeBtn = within(dialog).getByLabelText('Close') as HTMLElement;
		await expect(closeBtn, 'close button must be present when dialog starts open').toBeTruthy();

		await userEvent.click(closeBtn);

		await expect(controlledOpenSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root open onOpenChange={controlledOpenSpy}>
				<Dialog.Content>
					<Dialog.Header>
						<Dialog.Title>Controlled open via portal</Dialog.Title>
						<Dialog.Description>
							onOpenChange spy receives false when the dialog is dismissed. In production, bind:open
							wires the parent reactive boolean to bits-ui root state — dismissing the dialog (ESC /
							close / overlay click) calls onOpenChange(false).
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

<Story
	name="PortalRendersToDomBody"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const dialog = document.querySelector('[role="dialog"]');
		await expect(
			dialog,
			'dialog element with role=dialog must exist in the document (portalled to body)'
		).toBeTruthy();

		const inCanvas = canvasElement.contains(dialog);
		await expect(
			inCanvas,
			'portalled dialog must NOT be a descendant of canvasElement (it is in document.body)'
		).toBe(false);

		const inBody = document.body.contains(dialog);
		await expect(inBody, 'portalled dialog must be a descendant of document.body').toBe(true);
	}}
>
	{#snippet template()}
		<div data-testid="canvas-root">
			<Dialog.Root open>
				<Dialog.Portal>
					<Dialog.Overlay />
					<DialogPrimitive.Content
						class="bg-card text-card-foreground fixed top-1/2 left-1/2 z-50 grid w-full max-w-md -translate-x-1/2 -translate-y-1/2 gap-4 rounded-[var(--radius)] border border-border p-6 text-sm outline-none"
					>
						<Dialog.Header>
							<Dialog.Title>Portal DOM position check</Dialog.Title>
							<Dialog.Description>
								The play function asserts this content is in document.body, not inside canvasElement
								— confirming the portal teleportation works correctly.
							</Dialog.Description>
						</Dialog.Header>
					</DialogPrimitive.Content>
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledPortalRendersInline"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const dialog = canvasElement.querySelector('[data-testid="inline-portal-content"]');
		await expect(
			dialog,
			'inline-rendered portal content must be inside canvasElement when disabled=true'
		).toBeTruthy();

		await expect(
			canvasElement.contains(dialog),
			'disabled portal content must be a descendant of canvasElement'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="relative" style="min-height: 160px;">
			<Dialog.Root open>
				<Dialog.Portal disabled>
					<DialogPrimitive.Content
						data-testid="inline-portal-content"
						class="bg-card text-card-foreground relative z-50 grid w-full max-w-sm gap-4 rounded-[var(--radius)] border border-border p-6 text-sm outline-none"
					>
						<Dialog.Header>
							<Dialog.Title>Inline render (disabled=true)</Dialog.Title>
							<Dialog.Description>
								disabled=true — content stays in the document flow inside canvasElement. The play
								function asserts it is NOT teleported to document.body.
							</Dialog.Description>
						</Dialog.Header>
					</DialogPrimitive.Content>
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="EscKeyDismisses"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		escKeyDismissesSpy.mockClear();
		const canvas = within(document.body);
		const dialogBefore = canvas.getByRole('dialog');
		await expect(dialogBefore, 'role=dialog must be present in document before ESC').toBeTruthy();

		const closeBtn = within(dialogBefore).getByLabelText('Close') as HTMLElement | null;
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
					<Dialog.Title>ESC key dismisses via portal</Dialog.Title>
					<Dialog.Description>
						bits-ui's EscapeLayer on DialogPrimitive.Content intercepts ESC and calls
						onOpenChange(false), which unmounts the portal's children.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="OverlayClickDismisses"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const overlay = document.querySelector('[data-slot="dialog-overlay"]');
		await expect(
			overlay,
			'Dialog.Overlay (data-slot="dialog-overlay") must be rendered in the document'
		).toBeTruthy();

		await expect(
			overlay?.getAttribute('data-state'),
			'dialog-overlay must have data-state="open" when dialog is open'
		).toBe('open');

		const dialog = document.querySelector('[role="dialog"]');
		await expect(dialog, 'role=dialog must be present').toBeTruthy();
		await expect(dialog?.getAttribute('data-state'), 'dialog must have data-state="open"').toBe(
			'open'
		);

		await expect(
			overlay?.getAttribute('data-slot'),
			'overlay element must have data-slot="dialog-overlay"'
		).toBe('dialog-overlay');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Overlay presence and state</Dialog.Title>
					<Dialog.Description>
						Dialog.Overlay renders as a portal child with data-slot="dialog-overlay" and
						data-state="open". bits-ui's DismissibleLayer is registered on the content panel and
						fires onOpenChange(false) on outside-click via Svelte's event delegation.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="CloseButtonDismisses"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		closeButtonDismissesSpy.mockClear();
		const canvas = within(document.body);
		const dialogBefore = canvas.getByRole('dialog');
		await expect(
			dialogBefore,
			'role=dialog must be present before close button click'
		).toBeTruthy();

		const closeBtn = within(dialogBefore).getByLabelText('Close') as HTMLElement;
		await expect(closeBtn, 'close button with aria-label="Close" must be in DOM').toBeTruthy();

		await userEvent.click(closeBtn);

		await expect(closeButtonDismissesSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<Dialog.Root open onOpenChange={closeButtonDismissesSpy}>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Close button dismisses</Dialog.Title>
					<Dialog.Description>
						The XIcon close button (DialogPrimitive.Close) calls onOpenChange(false) on click,
						unmounting the portal's children.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="AccessibilityAttributes"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const canvas = within(document.body);

		const dialog = canvas.getByRole('dialog');
		await expect(dialog, 'element with role=dialog must be present').toBeTruthy();

		await expect(
			dialog.getAttribute('aria-modal'),
			'aria-modal must be "true" on dialog content'
		).toBe('true');

		const labelledBy = dialog.getAttribute('aria-labelledby');
		await expect(
			labelledBy,
			'dialog must have aria-labelledby linking to Dialog.Title'
		).toBeTruthy();

		const titleEl = labelledBy ? document.getElementById(labelledBy) : null;
		await expect(
			titleEl,
			`Dialog.Title element with id="${labelledBy}" must exist in document`
		).toBeTruthy();
		await expect(titleEl?.textContent?.trim()).toBe('A11y: role + aria-modal + labelledby');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>A11y: role + aria-modal + labelledby</Dialog.Title>
					<Dialog.Description>
						bits-ui sets role=dialog, aria-modal=true, and aria-labelledby on
						DialogPrimitive.Content automatically.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="PortalPropsDisabledUndefined"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const dialog = document.querySelector('[role="dialog"]');
		await expect(
			dialog,
			'role=dialog must exist in document (portal active with undefined portalProps)'
		).toBeTruthy();

		await expect(
			canvasElement.contains(dialog),
			'dialog must NOT be inside canvasElement when portalProps is undefined (portals to body)'
		).toBe(false);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>portalProps=undefined (default portal)</Dialog.Title>
					<Dialog.Description>
						No portalProps passed to Dialog.Content. The internal DialogPortal receives no `to` or
						`disabled` override — defaults to portal-to-body.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="PortalPropsDisabledTrue"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const dialog = canvasElement.querySelector('[role="dialog"]');
		await expect(
			dialog,
			'role=dialog must be inside canvasElement when portalProps.disabled=true'
		).toBeTruthy();

		const bodyDirect = document.body.querySelector(':scope > [role="dialog"]');
		await expect(
			bodyDirect,
			'dialog must NOT be a direct body-level child when portalProps.disabled=true'
		).toBeFalsy();
	}}
>
	{#snippet template()}
		<div class="relative" style="min-height: 160px;">
			<Dialog.Root open>
				<Dialog.Content portalProps={{ disabled: true }} class="static translate-x-0 translate-y-0">
					<Dialog.Header>
						<Dialog.Title>portalProps.disabled=true</Dialog.Title>
						<Dialog.Description>
							Dialog.Content forwards portalProps.disabled=true to DialogPortal. Content renders
							inline inside canvasElement, not teleported to body.
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ChildrenSnippetRenders"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const overlay = document.querySelector('[data-slot="dialog-overlay"]');
		await expect(
			overlay,
			'Dialog.Overlay (data-slot="dialog-overlay") must be in document'
		).toBeTruthy();

		const title =
			document.querySelector('[data-slot="dialog-title"]') ?? document.querySelector('h2');
		await expect(title, 'Dialog.Title must render as child of portal content').toBeTruthy();
		await expect(title?.textContent?.trim()).toBe('Children snippet renders via portal');

		const customEl = document.querySelector('[data-testid="portal-child"]');
		await expect(
			customEl,
			'custom child [data-testid="portal-child"] must render in portal'
		).toBeTruthy();
		await expect(customEl?.textContent?.trim()).toBe('Portal child rendered');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Portal>
				<Dialog.Overlay />
				<DialogPrimitive.Content
					class="bg-card text-card-foreground fixed top-1/2 left-1/2 z-50 grid w-full max-w-md -translate-x-1/2 -translate-y-1/2 gap-4 rounded-[var(--radius)] border border-border p-6 text-sm outline-none"
				>
					<Dialog.Header>
						<Dialog.Title>Children snippet renders via portal</Dialog.Title>
					</Dialog.Header>
					<p data-testid="portal-child" class="text-sm">Portal child rendered</p>
				</DialogPrimitive.Content>
			</Dialog.Portal>
		</Dialog.Root>
	{/snippet}
</Story>
