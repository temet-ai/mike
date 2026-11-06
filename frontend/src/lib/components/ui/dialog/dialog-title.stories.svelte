<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import * as Dialog from './index';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';

	const { Story } = defineMeta({
		title: 'UI/Dialog/DialogTitle',
		component: Dialog.Title,
		tags: ['autodocs'],
		argTypes: {
			class: { control: 'text' },
			level: {
				control: 'select',
				options: [1, 2, 3, 4, 5, 6]
			}
		}
	});
</script>

<Story name="Default">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Confirm deletion</Dialog.Title>
					<Dialog.Description>
						Default title: no level prop, plain text, styled as the guide's machine label (machine,
						--text-xs, --text-1, leading-none).
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithLevel1">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title level={1}>Level 1 heading</Dialog.Title>
					<Dialog.Description>
						Title with level=1: the most prominent heading level inside a dialog. bits-ui passes the
						level prop to DialogPrimitive.Title.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithLevel2">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title level={2}>Level 2 heading</Dialog.Title>
					<Dialog.Description>
						Title with level=2: an alternative heading level for dialogs nested in a page that
						already contains h1 content.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="PlainText">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Plain text title</Dialog.Title>
					<Dialog.Description>
						Minimal case: plain text children, no level prop, no custom class, no markup. Confirms
						the base rendering path without any prop variation.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithMarkupChildren">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>
						Delete <em>selected</em> session —
						<span class="text-[var(--destructive)]">irreversible</span>
					</Dialog.Title>
					<Dialog.Description>
						Title with markup children: em, span with custom color class. Inline markup renders
						without stripping inside the title element.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="LongTitle">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>
						Permanently remove all captured request data from this proxy session and reset the
						session store to its initial empty state
					</Dialog.Title>
					<Dialog.Description>
						Long title: multi-line wrapping without truncation. The title wraps within the
						sm:max-w-lg Dialog.Content constraint and the header grows vertically.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="CustomClass">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title class="text-lg">Custom class title</Dialog.Title>
					<Dialog.Description>
						Title with class="text-lg": cn() merges text-lg with the default machine label classes.
						The custom class overrides the default font size via tailwind-merge.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="TitleOnly">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Title without description</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DomAssertDataSlot"
	tags={['interactive']}
	play={async ({ canvasElement: _canvasElement }) => {
		const title = document.body.querySelector('[data-slot="dialog-title"]');
		await expect(
			title,
			'dialog-title element with data-slot="dialog-title" must exist'
		).toBeTruthy();

		await expect(title?.classList.contains('machine'), 'machine class must be present').toBe(true);
		await expect(
			title?.classList.contains('text-[length:var(--text-xs)]'),
			'--text-xs size class must be present'
		).toBe(true);
		await expect(
			title?.classList.contains('leading-none'),
			'leading-none class must be present'
		).toBe(true);
		await expect(
			title?.classList.contains('text-[color:var(--text-1)]'),
			'--text-1 colour class must be present'
		).toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Data-slot assertion</Dialog.Title>
					<Dialog.Description>
						The play function checks data-slot="dialog-title" and the default class list: machine,
						text-[length:var(--text-xs)], leading-none, text-[color:var(--text-1)].
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DomAssertTextContent"
	tags={['interactive']}
	play={async ({ canvasElement: _canvasElement }) => {
		const title = document.body.querySelector('[data-slot="dialog-title"]');
		await expect(title, 'dialog-title must be in DOM (portalled into body)').toBeTruthy();
		await expect(
			title?.textContent?.trim(),
			'title text content must match the provided children'
		).toBe('Text content assertion title');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Text content assertion title</Dialog.Title>
					<Dialog.Description>
						The play function asserts the title's textContent matches the provided children string.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DomAssertClassMerge"
	tags={['interactive']}
	play={async ({ canvasElement: _canvasElement }) => {
		const title = document.body.querySelector('[data-slot="dialog-title"]');
		await expect(title, 'dialog-title must be in DOM').toBeTruthy();

		await expect(
			title?.classList.contains('machine'),
			'default machine class must survive cn() merge'
		).toBe(true);
		await expect(
			title?.classList.contains('text-[color:var(--text-1)]'),
			'default --text-1 colour class must survive cn() merge'
		).toBe(true);

		await expect(
			title?.classList.contains('italic'),
			'injected italic class must be present after cn() merge'
		).toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title class="italic">Class merge assertion title</Dialog.Title>
					<Dialog.Description>
						Both the custom italic class and the default machine / --text-1 classes must coexist
						after cn() merge.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DomAssertRefBinding"
	tags={['interactive']}
	play={async ({ canvasElement: _canvasElement }) => {
		const title = document.body.querySelector('[data-slot="dialog-title"]');
		await expect(title, 'dialog-title DOM node must be present in portalled body').toBeTruthy();
		await expect(
			title instanceof HTMLElement,
			'dialog-title must be an HTMLElement (ref binding target)'
		).toBe(true);
		const tag = title?.tagName.toLowerCase() ?? '';
		await expect(
			['div', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6'].includes(tag),
			`dialog-title must be a heading or div element, got "${tag}"`
		).toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Ref binding title</Dialog.Title>
					<Dialog.Description>
						The ref prop is bound to the underlying DialogPrimitive.Title DOM element. The play
						function asserts that the bound node is a real HTMLElement.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DomAssertRestProps"
	tags={['interactive']}
	play={async ({ canvasElement: _canvasElement }) => {
		const title = document.body.querySelector('[data-slot="dialog-title"]');
		await expect(title, 'dialog-title must be in DOM (portalled into body)').toBeTruthy();

		await expect(
			title?.getAttribute('data-testid'),
			'data-testid from restProps must be forwarded to the DOM element'
		).toBe('my-title');

		await expect(
			title?.getAttribute('id'),
			'id from restProps must be forwarded to the DOM element'
		).toBe('custom-title-id');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title data-testid="my-title" id="custom-title-id">
						RestProps assertion title
					</Dialog.Title>
					<Dialog.Description>
						data-testid and id are passed via restProps spread to the underlying
						DialogPrimitive.Title element.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveLevel"
	tags={['interactive']}
	args={{ level: 3, onclick: fn() }}
	play={async ({ canvasElement: _canvasElement, args }) => {
		const title = document.body.querySelector('[data-slot="dialog-title"]');
		await expect(title, 'dialog-title must be in portalled body').toBeTruthy();

		const tag = title?.tagName.toLowerCase() ?? '';
		const ariaLevel = title?.getAttribute('aria-level');
		const isH3 = tag === 'h3';
		const hasAriaLevel3 = ariaLevel === '3';
		await expect(
			isH3 || hasAriaLevel3,
			`level=3 must produce tag h3 or aria-level="3", got tag="${tag}" aria-level="${ariaLevel}"`
		).toBe(true);

		await userEvent.click(title as Element);
		await expect(args.onclick).toHaveBeenCalledOnce();
	}}
>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title level={args.level} onclick={args.onclick}>
						Level {args.level} heading
					</Dialog.Title>
					<Dialog.Description>
						Heading level {args.level}: the play function verifies the rendered tag or aria-level
						matches the prop value, then clicks the title to confirm onclick spy forwarding.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveClick"
	tags={['interactive']}
	args={{ onclick: fn() }}
	play={async ({ canvasElement: _canvasElement, args }) => {
		const title = document.body.querySelector('[data-slot="dialog-title"]');
		await expect(title, 'dialog-title must be present in portalled body').toBeTruthy();

		await userEvent.click(title as Element);

		await expect(args.onclick).toHaveBeenCalledOnce();
	}}
>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title onclick={args.onclick}>
						Click me to verify restProps forwarding
					</Dialog.Title>
					<Dialog.Description>
						onclick is passed as a restProps arg. The play function clicks the title and asserts the
						fn() spy was called, confirming the restProps forwarding pipeline is intact.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="SnapshotDefault"
	tags={['visual']}
	play={async ({ canvasElement: _canvasElement }) => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-title/Default', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Delete session data</Dialog.Title>
					<Dialog.Description>
						This action cannot be undone. All captured requests and metadata for this session will
						be permanently removed from the proxy store.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ThemeForge"
	tags={['visual']}
	play={async ({ canvasElement: _canvasElement }) => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-title/ThemeForge', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-theme="forge">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Forge theme</Dialog.Title>
						<Dialog.Description>
							Primary heading text on the forge theme surface. The --foreground token (--text-1)
							renders on the --surface-card background in the forge (default dark) theme.
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ThemeGlacier"
	tags={['visual']}
	play={async ({ canvasElement: _canvasElement }) => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-title/ThemeGlacier', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-theme="glacier">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Glacier theme</Dialog.Title>
						<Dialog.Description>
							Primary heading text on the glacier theme surface. The --foreground token (--text-1)
							renders on the --surface-card background in the glacier (cool blue-grey) theme.
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ThemeVoid"
	tags={['visual']}
	play={async ({ canvasElement: _canvasElement }) => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-title/ThemeVoid', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-theme="void">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Void theme</Dialog.Title>
						<Dialog.Description>
							Primary heading text on the void theme surface. The --foreground token (--text-1)
							renders on the near-black --surface-card in the void theme (violet-tinted heading
							text).
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ThemePhosphor"
	tags={['visual']}
	play={async ({ canvasElement: _canvasElement }) => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-title/ThemePhosphor', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-theme="phosphor">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Phosphor theme</Dialog.Title>
						<Dialog.Description>
							Primary heading text on the phosphor theme surface. The --foreground token (--text-1)
							renders on the near-pure-black --surface-card in the phosphor theme (CRT green
							terminal aesthetic).
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ThemeCrimson"
	tags={['visual']}
	play={async ({ canvasElement: _canvasElement }) => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-title/ThemeCrimson', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-theme="crimson">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Crimson theme</Dialog.Title>
						<Dialog.Description>
							Primary heading text on the crimson theme surface. The --foreground token (--text-1)
							renders on the warm-dark brown --surface-card in the crimson theme.
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual', 'design-exploration']}
	play={async ({ canvasElement: _canvasElement }) => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-title/AllThemes');
	}}
>
	{#snippet template()}
		<div style="display:flex; flex-direction:column; gap:16px; padding:8px;">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div data-theme={theme}>
					<Dialog.Root open>
						<Dialog.Content showCloseButton={false} class="sm:max-w-md">
							<Dialog.Header>
								<Dialog.Title>{theme} theme</Dialog.Title>
								<Dialog.Description>
									Title on {theme}: --foreground (--text-1) on --surface-card.
								</Dialog.Description>
							</Dialog.Header>
						</Dialog.Content>
					</Dialog.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
