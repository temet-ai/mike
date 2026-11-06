<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, userEvent, within, expect, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Sheet from './index';
	import { Button } from '$lib/components/ui/button';

	const { Story } = defineMeta({
		title: 'UI/Sheet/SheetTrigger',
		component: Sheet.Trigger,
		tags: ['autodocs'],
		argTypes: {
			type: {
				control: 'select',
				options: ['button', 'submit', 'reset'],
				description: 'HTML button type attribute. Defaults to "button".'
			}
		}
	});

	const onOpenChangeDisabled = fn();
	const onOpenChangeClick = fn();
	const onOpenChangeKeyboard = fn();
	const onOpenChangeRef = fn();

	let triggerRefForDemo: HTMLButtonElement | null = null;
</script>

<Story name="DefaultButton">
	{#snippet template()}
		<div class="flex h-[300px] items-center justify-center bg-[var(--surface-bg)]">
			<Sheet.Root>
				<Sheet.Trigger>Open sheet</Sheet.Trigger>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Sheet opened</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							This sheet was opened by the default trigger button.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithChildSnippet">
	{#snippet template()}
		<div class="flex h-[300px] items-center justify-center bg-[var(--surface-bg)]">
			<Sheet.Root>
				<Sheet.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open side panel</Button>
					{/snippet}
				</Sheet.Trigger>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Side panel</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							Trigger used the bits-ui child snippet to pass props to a styled Button.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledTrigger"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		onOpenChangeDisabled.mockClear();

		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /open sheet/i });
		await expect(
			trigger.hasAttribute('disabled'),
			'trigger should carry the disabled attribute'
		).toBe(true);

		await userEvent.click(trigger, { pointerEventsCheck: 0 });

		await expect(onOpenChangeDisabled).not.toHaveBeenCalled();

		const content = document.body.querySelector('[data-slot="sheet-content"]');
		await expect(content, 'sheet-content must not be portaled when trigger is disabled').toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex h-[300px] items-center justify-center bg-[var(--surface-bg)]">
			<Sheet.Root onOpenChange={onOpenChangeDisabled}>
				<Sheet.Trigger disabled>
					{#snippet child({ props })}
						<Button variant="outline" {...props} disabled>Open sheet (disabled)</Button>
					{/snippet}
				</Sheet.Trigger>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Unreachable</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">This sheet should not open.</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AsFormSubmit"
	tags={['interactive']}
	args={{ type: 'submit' }}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /submit/i });
		await expect(
			trigger.getAttribute('type'),
			'trigger button must have type="submit" in the DOM'
		).toBe(args.type);
	}}
>
	{#snippet template(args)}
		<div class="flex h-[300px] items-center justify-center bg-[var(--surface-bg)]">
			<Sheet.Root>
				<form>
					<Sheet.Trigger type={args.type}>
						{#snippet child({ props })}
							<Button variant="default" type={args.type} {...props}>Submit &amp; open sheet</Button>
						{/snippet}
					</Sheet.Trigger>
				</form>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Submit triggered</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							Trigger used type="{args.type}". Sheet opens on button activation.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		onOpenChangeRef.mockClear();

		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /open sheet/i });

		await expect(
			trigger.getAttribute('data-slot'),
			'trigger must carry data-slot="sheet-trigger" (set by sheet-trigger.svelte)'
		).toBe('sheet-trigger');

		await userEvent.click(trigger);

		await expect(onOpenChangeRef).toHaveBeenCalledWith(true);

		await waitFor(() => {
			const content = document.body.querySelector('[data-slot="sheet-content"]');
			expect(
				content?.getAttribute('data-state'),
				'sheet-content should have data-state="open" after trigger click via ref-bound element'
			).toBe('open');
		});
	}}
>
	{#snippet template()}
		<div class="flex h-[300px] items-center justify-center bg-[var(--surface-bg)]">
			<Sheet.Root onOpenChange={onOpenChangeRef}>
				<Sheet.Trigger bind:ref={triggerRefForDemo}>Open sheet (ref bound)</Sheet.Trigger>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Ref binding demo</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							The trigger's ref prop was bound — see play() for the assertion.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="OpensOnClick"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		onOpenChangeClick.mockClear();

		const canvas = within(canvasElement);

		await waitFor(() => {
			const btn = canvas.getByRole('button', { name: /open sheet/i });
			expect(btn, 'trigger button should be present and interactive').toBeTruthy();
		});

		const trigger = canvas.getByRole('button', { name: /open sheet/i });

		await userEvent.click(trigger, { pointerEventsCheck: 0 });

		await expect(onOpenChangeClick).toHaveBeenCalledWith(true);

		await waitFor(() => {
			const content = document.body.querySelector('[data-slot="sheet-content"]');
			expect(
				content?.getAttribute('data-state'),
				'sheet-content should have data-state="open" after trigger click'
			).toBe('open');
		});
	}}
>
	{#snippet template()}
		<div class="flex h-[300px] items-center justify-center bg-[var(--surface-bg)]">
			<Sheet.Root onOpenChange={onOpenChangeClick}>
				<Sheet.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open sheet</Button>
					{/snippet}
				</Sheet.Trigger>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Opened by trigger</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							This sheet opened because the trigger was clicked.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardOpen"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		onOpenChangeKeyboard.mockClear();

		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /keyboard open/i });

		trigger.focus();
		await expect(document.activeElement, 'trigger should receive focus').toBe(trigger);

		await userEvent.keyboard('{Enter}');

		await expect(onOpenChangeKeyboard).toHaveBeenCalledWith(true);

		const content = document.body.querySelector('[data-slot="sheet-content"]');
		await expect(
			content?.getAttribute('data-state'),
			'sheet-content should have data-state="open" after Enter key'
		).toBe('open');
	}}
>
	{#snippet template()}
		<div class="flex h-[300px] items-center justify-center bg-[var(--surface-bg)]">
			<Sheet.Root onOpenChange={onOpenChangeKeyboard}>
				<Sheet.Trigger>Keyboard open</Sheet.Trigger>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Opened by keyboard</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							This sheet opened because Enter was pressed on the trigger.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-trigger/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-4 p-4 bg-[var(--surface-bg)]">
			<p class="text-[var(--text-2)] text-xs font-mono">
				SheetTrigger (outline Button) — all 5 themes
			</p>
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div
					data-theme={theme}
					class="flex items-center gap-3 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-card)] px-4 py-3"
				>
					<span class="text-[var(--text-2)] text-xs font-mono w-20">{theme}</span>
					<Sheet.Root>
						<Sheet.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Open panel</Button>
							{/snippet}
						</Sheet.Trigger>
						<Sheet.Content aria-describedby={undefined}>
							<div class="flex flex-col gap-4 p-5">
								<Sheet.Title>{theme} panel</Sheet.Title>
							</div>
						</Sheet.Content>
					</Sheet.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
