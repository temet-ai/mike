<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import * as Dialog from './index';
	import { Button } from '$lib/components/ui/button';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import DialogTrigger from './dialog-trigger.svelte';

	const { Story } = defineMeta({
		title: 'UI/Dialog/DialogTrigger',
		component: DialogTrigger,
		tags: ['autodocs'],
		parameters: {
			backgrounds: { disable: true }
		}
	});

	let controlledOpen = $state(false);

	let boundTriggerRef = $state<HTMLButtonElement | null>(null);

	let bindOpenTestOpen = $state(false);
</script>

<Story name="Default">
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open Dialog</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Default trigger</Dialog.Title>
						<Dialog.Description>
							This dialog was opened by a text-label trigger button.
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

<Story name="IconOnly">
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger aria-label="Search">
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Search" {...props}>
							<SearchIcon class="size-4" aria-hidden="true" />
						</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Icon-only trigger</Dialog.Title>
						<Dialog.Description
							>Opened via an icon-only trigger with aria-label="Search".</Dialog.Description
						>
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

<Story name="DisabledTrigger">
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger disabled>
					{#snippet child({ props })}
						<Button variant="outline" disabled {...props}>Open Dialog (disabled)</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Disabled trigger</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story name="ControlledOpen">
	{#snippet template()}
		<div class="flex flex-col items-center gap-4 p-8">
			<p class="text-muted-foreground text-sm">
				State: {controlledOpen ? 'open' : 'closed'}
			</p>
			<Dialog.Root bind:open={controlledOpen}>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Toggle Dialog</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Controlled open</Dialog.Title>
						<Dialog.Description>
							The open state is two-way bound. Dismissing writes false back through bind:open.
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

<Story name="AsFormButton">
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger type="submit">
					{#snippet child({ props })}
						<Button variant="default" type="submit" {...props}>Submit (opens dialog)</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Form-type trigger</Dialog.Title>
						<Dialog.Description>Trigger has type="submit" for form integration.</Dialog.Description>
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

<Story name="AllThemes" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex flex-wrap gap-6 p-4">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div data-theme={theme} class="rounded-lg border border-border bg-card p-4">
					<p class="text-foreground mb-3 text-xs font-medium">{theme}</p>
					<Dialog.Root>
						<Dialog.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" {...props}>Open ({theme})</Button>
							{/snippet}
						</Dialog.Trigger>
						<Dialog.Content class="sm:max-w-xs">
							<Dialog.Header>
								<Dialog.Title>Theme: {theme}</Dialog.Title>
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
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="DataSlotAttribute"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector('[data-slot="dialog-trigger"]');
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();
		await expect(
			trigger?.getAttribute('data-slot'),
			'data-slot attribute value must equal "dialog-trigger"'
		).toBe('dialog-trigger');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open Dialog</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Data slot check</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="TypePropPassthrough"
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector('[data-slot="dialog-trigger"]');
		await expect(trigger, 'trigger should exist').toBeTruthy();
		await expect(trigger?.getAttribute('type'), 'trigger type should default to "button"').toBe(
			'button'
		);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Check type</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Type passthrough check</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SubmitTypePropPassthrough"
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector('[data-slot="dialog-trigger"]');
		await expect(trigger, 'trigger should exist').toBeTruthy();
		await expect(trigger?.getAttribute('type'), 'trigger type should be "submit" when set').toBe(
			'submit'
		);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger type="submit">
					{#snippet child({ props })}
						<Button variant="default" type="submit" {...props}>Submit type</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Submit type passthrough</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledPropPassthrough"
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLButtonElement | null;
		await expect(trigger, 'trigger should exist').toBeTruthy();
		await expect(trigger?.disabled, 'trigger should have disabled attribute').toBe(true);
		const content = canvasElement.querySelector('[data-slot="dialog-content"]');
		await expect(
			content,
			'dialog content should not be in DOM when trigger is disabled'
		).toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger disabled>
					{#snippet child({ props })}
						<Button variant="outline" disabled {...props}>Disabled trigger</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Disabled passthrough check</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ClickOpensThenCloses"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLElement | null;
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();

		const closeBefore = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBefore, 'dialog close button should not exist before click').toBeNull();

		await userEvent.click(trigger!);

		const closeAfter = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(
			closeAfter,
			'dialog close button should appear in portal after trigger click'
		).toBeTruthy();

		await userEvent.click(closeAfter as HTMLElement);

		await waitFor(() => {
			expect(
				document.body.querySelector('[data-slot="dialog-close"]'),
				'dialog close button should be gone after clicking close'
			).toBeNull();
		});
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open Dialog</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Click opens then closes</Dialog.Title>
						<Dialog.Description>Click the close button to dismiss.</Dialog.Description>
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
	name="KeyboardOpenEnter"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLElement | null;
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();

		trigger!.focus();
		await userEvent.keyboard('{Enter}');

		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(
			closeBtn,
			'dialog close button should appear after Enter key on trigger'
		).toBeTruthy();

		await userEvent.keyboard('{Escape}');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Keyboard open (Enter)</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Keyboard opened via Enter</Dialog.Title>
						<Dialog.Description>This dialog was opened with the Enter key.</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardOpenSpace"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLElement | null;
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();

		trigger!.focus();
		await userEvent.keyboard('{ }');

		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(
			closeBtn,
			'dialog close button should appear after Space key on trigger'
		).toBeTruthy();

		await userEvent.keyboard('{Escape}');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Keyboard open (Space)</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Keyboard opened via Space</Dialog.Title>
						<Dialog.Description>This dialog was opened with the Space key.</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="EscKeyClosesDialog"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		await waitFor(() => {
			const btn = document.body.querySelector('[data-slot="dialog-close"]');
			if (!btn) throw new Error('dialog-close not yet in portal');
		});

		const closeBefore = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(
			closeBefore,
			'dialog close button should be present when dialog starts open'
		).toBeTruthy();

		await userEvent.keyboard('{Escape}');

		let closeAfterEsc = document.body.querySelector('[data-slot="dialog-close"]');
		if (closeAfterEsc) {
			await expect(
				closeAfterEsc.getAttribute('data-state'),
				'dialog-close must have data-state="closed" after ESC (not still "open")'
			).toBe('closed');
		} else {
			await expect(
				closeAfterEsc,
				'dialog-close should be null in DOM after ESC (fully dismissed)'
			).toBeNull();
		}
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root open>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>ESC key closes dialog</Dialog.Title>
						<Dialog.Description>Press ESC to close. Focus returns to trigger.</Dialog.Description>
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
	name="DisabledClickHasNoEffect"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLElement | null;
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();

		await userEvent.click(trigger!, { pointerEventsCheck: 0 });

		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(
			closeBtn,
			'dialog close button should NOT be in portal after clicking disabled trigger'
		).toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger disabled>
					{#snippet child({ props })}
						<Button variant="outline" disabled {...props}>Disabled (no click)</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Disabled click test</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomClassOverride"
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector('[data-slot="dialog-trigger"]');
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();
		await expect(trigger?.className, 'trigger should contain the custom class string').toContain(
			'custom-trigger-test-class'
		);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger class="custom-trigger-test-class">
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Custom class trigger</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Custom class check</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="OnClickHandlerFires"
	tags={['interactive']}
	args={{ onclick: fn() }}
	play={async ({ canvasElement, args }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLElement | null;
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();

		await userEvent.click(trigger!);

		await expect(args.onclick).toHaveBeenCalledOnce();

		await userEvent.keyboard('{Escape}');
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger onclick={args.onclick}>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Click handler test</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>onclick handler fired</Dialog.Title>
						<Dialog.Description
							>The onclick spy was called once on trigger click.</Dialog.Description
						>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="BindRefExercise"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		await expect(boundTriggerRef, 'boundTriggerRef should be non-null after mount').toBeTruthy();
		await expect(
			boundTriggerRef instanceof HTMLButtonElement,
			'boundTriggerRef must be an HTMLButtonElement'
		).toBe(true);

		const triggerInCanvas = canvasElement.querySelector('[data-slot="dialog-trigger"]');
		await expect(
			triggerInCanvas,
			'data-slot="dialog-trigger" should be in canvas DOM'
		).toBeTruthy();
		await expect(
			boundTriggerRef,
			'bind:ref must resolve to the same DOM node as [data-slot="dialog-trigger"]'
		).toBe(triggerInCanvas);

		await expect(
			(boundTriggerRef as HTMLButtonElement).disabled,
			'bind:ref element should not be disabled for this story'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger bind:ref={boundTriggerRef}>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Test bind:ref</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>bind:ref exercise</Dialog.Title>
						<Dialog.Description
							>Verifies the bind:ref prop resolves to the trigger HTMLButtonElement.</Dialog.Description
						>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="IconOnlyAriaLabel"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /open search/i });
		await expect(trigger, 'icon-only trigger must be findable by aria-label').toBeTruthy();
		await expect(trigger?.getAttribute('aria-label')).toBe('Open search');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger aria-label="Open search">
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Open search" {...props}>
							<SearchIcon class="size-4" aria-hidden="true" />
						</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Icon-only trigger a11y</Dialog.Title>
						<Dialog.Description>Trigger has aria-label for screen readers.</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AriaExpandedClosedState"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector('[data-slot="dialog-trigger"]');
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();
		const ariaExpanded = trigger?.getAttribute('aria-expanded');
		await expect(
			ariaExpanded,
			`aria-expanded should be "false" when dialog is closed, got "${ariaExpanded}"`
		).toBe('false');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Check aria-expanded</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Aria expanded state check</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AriaExpandedOpenState"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLElement | null;
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();

		await userEvent.click(trigger!);

		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(
			closeBtn,
			'dialog should be open after trigger click (close button in portal)'
		).toBeTruthy();

		const ariaExpanded = trigger?.getAttribute('aria-expanded');
		await expect(
			ariaExpanded,
			`aria-expanded should be "true" while dialog is open, got "${ariaExpanded}"`
		).toBe('true');

		await userEvent.keyboard('{Escape}');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open to check aria-expanded</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Aria expanded open check</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardTabNavigation"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLElement | null;
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();
		const tabIndex = trigger?.tabIndex ?? -1;
		await expect(
			tabIndex,
			`trigger tabIndex should be >= 0 (keyboard reachable), got ${tabIndex}`
		).toBeGreaterThanOrEqual(0);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Tab-reachable trigger</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Tab navigation check</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="BindOpenReactiveBooleanToggle"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		if (document.body.querySelector('[data-slot="dialog-close"]')) {
			await userEvent.keyboard('{Escape}');
			await waitFor(() => {
				const el = document.body.querySelector('[data-slot="dialog-close"]');
				if (el !== null) throw new Error('dialog-close still present after defensive ESC reset');
			});
		}

		const closeBefore = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBefore, 'dialog should start closed (no close button in portal)').toBeNull();

		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLElement | null;
		await expect(trigger, 'trigger should be in DOM').toBeTruthy();
		await waitFor(() => {
			if (getComputedStyle(trigger!).pointerEvents === 'none') {
				throw new Error(
					'trigger still has pointer-events: none (transition/lock not yet released)'
				);
			}
		});
		await userEvent.click(trigger!);

		await waitFor(() => {
			const el = document.body.querySelector('[data-slot="dialog-close"]');
			if (!el) throw new Error('dialog-close not yet in portal after trigger click');
		});
		const closeOpen = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeOpen, 'dialog should open after trigger click via bind:open').toBeTruthy();

		await userEvent.keyboard('{Escape}');
		await waitFor(() => {
			const el = document.body.querySelector('[data-slot="dialog-close"]');
			if (el !== null)
				throw new Error('dialog-close still in portal after ESC (exit animation pending)');
		});
		const closeClosed = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeClosed, 'dialog should close after ESC via bind:open').toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex flex-col items-center gap-4 p-8">
			<Dialog.Root bind:open={bindOpenTestOpen}>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open via trigger (tests bind:open)</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Bind:open toggle</Dialog.Title>
						<Dialog.Description
							>Dialog opened via trigger click; bits-ui writes open state back through bind:open.</Dialog.Description
						>
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
	name="DefaultVisualSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-trigger/Default');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open Dialog</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Visual snapshot baseline</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledVisualSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-trigger/Disabled');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger disabled>
					{#snippet child({ props })}
						<Button variant="outline" disabled {...props}>Open Dialog (disabled)</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Disabled trigger snapshot</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="IconOnlyVisualSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-trigger/IconOnly');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger aria-label="Open search">
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Open search" {...props}>
							<SearchIcon class="size-4" aria-hidden="true" />
						</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Icon-only trigger snapshot</Dialog.Title>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-trigger/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex flex-wrap gap-6 p-4">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div data-theme={theme} class="rounded-lg border border-border bg-card p-4">
					<p class="text-foreground mb-3 text-xs font-medium">{theme}</p>
					<Dialog.Root>
						<Dialog.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" {...props}>Open ({theme})</Button>
							{/snippet}
						</Dialog.Trigger>
						<Dialog.Content class="sm:max-w-xs">
							<Dialog.Header>
								<Dialog.Title>Theme: {theme}</Dialog.Title>
							</Dialog.Header>
						</Dialog.Content>
					</Dialog.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="OpenStateVisualSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="dialog-trigger"]'
		) as HTMLElement | null;
		if (trigger) {
			await userEvent.click(trigger);
		}
		await captureStorySnapshot(document.body as unknown as HTMLElement, 'dialog-trigger/OpenState');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Dialog.Root>
				<Dialog.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open Dialog</Button>
					{/snippet}
				</Dialog.Trigger>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>Open state baseline</Dialog.Title>
						<Dialog.Description>
							Snapshot captured with dialog open — trigger has aria-expanded=true and portal is
							visible.
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
