<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, userEvent, expect } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Dialog from './index';

	const { Story } = defineMeta({
		title: 'UI/Dialog/DialogDescription',
		component: Dialog.Description,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const content = document.body.querySelector('[data-slot="dialog-content"]');
		await expect(content, 'dialog-content must be portalled into document.body').toBeTruthy();
		await captureStorySnapshot(document.body, 'dialog-description/Default', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Confirm action</Dialog.Title>
					<Dialog.Description>
						This is the default dialog description with secondary text styling applied via
						text-muted-foreground and text-sm.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="CustomClass"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const desc = document.body.querySelector('[data-slot="dialog-description"]');
		await expect(desc, 'dialog-description element must be in DOM').toBeTruthy();
		await expect(
			desc?.classList.contains('font-bold'),
			'injected font-bold class must be present after cn() merge'
		).toBe(true);
		await expect(
			desc?.classList.contains('text-sm'),
			'default text-sm class must survive cn() merge'
		).toBe(true);
		await expect(
			desc?.classList.contains('text-muted-foreground'),
			'default text-muted-foreground class must survive cn() merge'
		).toBe(true);
		await captureStorySnapshot(document.body, 'dialog-description/CustomClass', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Custom class dialog</Dialog.Title>
					<Dialog.Description class="font-bold">
						This description receives a custom class. Both the default text-sm and the injected
						font-bold must be present — cn() merges, not replaces.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="WithLinks"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const content = document.body.querySelector('[data-slot="dialog-content"]');
		await expect(content, 'dialog-content must be portalled into document.body').toBeTruthy();
		await captureStorySnapshot(document.body, 'dialog-description/WithLinks', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Dialog with links</Dialog.Title>
					<Dialog.Description>
						Review the <a href="#docs">documentation</a> before proceeding. See also the
						<a href="#guide">usage guide</a> for additional context.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="WithLinksInteraction"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const desc = document.body.querySelector('[data-slot="dialog-description"]');
		await expect(desc, 'dialog-description must be in DOM (portalled into body)').toBeTruthy();
		const links = Array.from(desc?.querySelectorAll('a') ?? []);
		await expect(links.length, 'two anchor elements must be present inside description').toBe(2);
		await expect(
			links.map((a) => a.getAttribute('href')),
			'both link hrefs must be present'
		).toEqual(['#docs', '#guide']);

		const firstLink = links[0];
		await expect(firstLink, 'first anchor element must be defined').toBeTruthy();
		const clickSpy = fn();
		firstLink!.addEventListener('click', clickSpy);
		await userEvent.click(firstLink as HTMLElement);
		await expect(
			clickSpy,
			'click spy on first link must be called after userEvent.click'
		).toHaveBeenCalledTimes(1);
		firstLink!.removeEventListener('click', clickSpy);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Dialog with links</Dialog.Title>
					<Dialog.Description>
						Review the <a href="#docs">documentation</a> before proceeding. See also the
						<a href="#guide">usage guide</a> for additional context.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="LongContent"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const content = document.body.querySelector('[data-slot="dialog-content"]');
		await expect(content, 'dialog-content must be portalled into document.body').toBeTruthy();
		await captureStorySnapshot(document.body, 'dialog-description/LongContent', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Long description</Dialog.Title>
					<Dialog.Description>
						This description contains a substantial amount of text to verify that content wraps
						naturally within the Dialog.Content bounds. The dialog uses a max-w-lg constraint and
						the description text flows across multiple lines without truncation. Every word is
						visible — there is no ellipsis, no line-clamp, no overflow-hidden clipping. The text
						simply wraps and the dialog grows vertically to accommodate it.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="EmptyContent"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const desc = document.body.querySelector('[data-slot="dialog-description"]');
		await expect(desc, 'dialog-description must render even with no text content').toBeTruthy();
		await expect(
			desc?.textContent?.trim(),
			'empty description text content must be empty string'
		).toBe('');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Empty description dialog</Dialog.Title>
					<Dialog.Description></Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const desc = document.body.querySelector('[data-slot="dialog-description"]');
		await expect(desc, 'dialog-description DOM node must be present').toBeTruthy();
		await expect(
			desc instanceof HTMLElement,
			'dialog-description must be an HTMLElement (ref binding target)'
		).toBe(true);
		await expect(
			['div', 'p'].includes(desc?.tagName.toLowerCase() ?? ''),
			`underlying DialogPrimitive.Description renders as a block element (div or p), got "${desc?.tagName.toLowerCase()}"`
		).toBe(true);
		await expect(
			desc?.getAttribute('data-slot'),
			'data-slot="dialog-description" must be on the bound element'
		).toBe('dialog-description');
		const clickSpy = fn();
		desc!.addEventListener('click', clickSpy);
		await userEvent.click(desc as HTMLElement);
		await expect(
			clickSpy,
			'click spy on bound ref element must be called after userEvent.click'
		).toHaveBeenCalledTimes(1);
		desc!.removeEventListener('click', clickSpy);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Ref binding dialog</Dialog.Title>
					<Dialog.Description>
						The ref prop is bound to the underlying DialogPrimitive.Description DOM element. The
						play function asserts that the bound node is a real HTMLElement that receives click
						events, confirming it is not a detached or proxy node.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DataAttributes"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const desc = document.body.querySelector('[data-slot="dialog-description"]');
		await expect(desc, 'dialog-description must be in DOM (portalled into body)').toBeTruthy();
		await expect(
			desc?.getAttribute('data-testid'),
			'data-testid from restProps must be forwarded to the DOM element'
		).toBe('my-description');
		await expect(
			desc?.getAttribute('aria-live'),
			'aria-live from restProps must be forwarded to the DOM element'
		).toBe('polite');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>RestProps dialog</Dialog.Title>
					<Dialog.Description data-testid="my-description" aria-live="polite">
						This description forwards data-testid and aria-live via the restProps spread to the
						underlying DialogPrimitive.Description element.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ThemeForge"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const content = document.body.querySelector('[data-slot="dialog-content"]');
		await expect(
			content,
			'ThemeForge: dialog-content must be in DOM (portalled into themed container)'
		).toBeTruthy();
		await captureStorySnapshot(document.body, 'dialog-description/ThemeForge', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-story-theme="forge" data-theme="forge">
			<Dialog.Root open>
				<Dialog.Content class="sm:max-w-lg" portalProps={{ to: '[data-story-theme="forge"]' }}>
					<Dialog.Header>
						<Dialog.Title>Forge theme</Dialog.Title>
						<Dialog.Description>
							Secondary text on the forge theme surface. The --muted-foreground token (--text-2)
							renders on the --surface-card background. Links on hover read --foreground (--text-1).
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ThemeGlacier"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const content = document.body.querySelector('[data-slot="dialog-content"]');
		await expect(
			content,
			'ThemeGlacier: dialog-content must be in DOM (portalled into themed container)'
		).toBeTruthy();
		await captureStorySnapshot(document.body, 'dialog-description/ThemeGlacier', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-story-theme="glacier" data-theme="glacier">
			<Dialog.Root open>
				<Dialog.Content class="sm:max-w-lg" portalProps={{ to: '[data-story-theme="glacier"]' }}>
					<Dialog.Header>
						<Dialog.Title>Glacier theme</Dialog.Title>
						<Dialog.Description>
							Secondary text on the glacier theme surface. The --muted-foreground token (--text-2)
							renders on the --surface-card background. Links on hover read --foreground (--text-1).
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ThemeVoid"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const content = document.body.querySelector('[data-slot="dialog-content"]');
		await expect(
			content,
			'ThemeVoid: dialog-content must be in DOM (portalled into themed container)'
		).toBeTruthy();
		await captureStorySnapshot(document.body, 'dialog-description/ThemeVoid', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-story-theme="void" data-theme="void">
			<Dialog.Root open>
				<Dialog.Content class="sm:max-w-lg" portalProps={{ to: '[data-story-theme="void"]' }}>
					<Dialog.Header>
						<Dialog.Title>Void theme</Dialog.Title>
						<Dialog.Description>
							Secondary text on the void theme surface. The --muted-foreground token (--text-2)
							renders on the --surface-card background. Links on hover read --foreground (--text-1).
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ThemePhosphor"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const content = document.body.querySelector('[data-slot="dialog-content"]');
		await expect(
			content,
			'ThemePhosphor: dialog-content must be in DOM (portalled into themed container)'
		).toBeTruthy();
		await captureStorySnapshot(document.body, 'dialog-description/ThemePhosphor', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-story-theme="phosphor" data-theme="phosphor">
			<Dialog.Root open>
				<Dialog.Content class="sm:max-w-lg" portalProps={{ to: '[data-story-theme="phosphor"]' }}>
					<Dialog.Header>
						<Dialog.Title>Phosphor theme</Dialog.Title>
						<Dialog.Description>
							Secondary text on the phosphor theme surface. The --muted-foreground token (--text-2)
							renders on the --surface-card background. Links on hover read --foreground (--text-1).
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ThemeCrimson"
	tags={['interactive', 'visual']}
	play={async ({ canvasElement }) => {
		const content = document.body.querySelector('[data-slot="dialog-content"]');
		await expect(
			content,
			'ThemeCrimson: dialog-content must be in DOM (portalled into themed container)'
		).toBeTruthy();
		await captureStorySnapshot(document.body, 'dialog-description/ThemeCrimson', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<div data-story-theme="crimson" data-theme="crimson">
			<Dialog.Root open>
				<Dialog.Content class="sm:max-w-lg" portalProps={{ to: '[data-story-theme="crimson"]' }}>
					<Dialog.Header>
						<Dialog.Title>Crimson theme</Dialog.Title>
						<Dialog.Description>
							Secondary text on the crimson theme surface. The --muted-foreground token (--text-2)
							renders on the --surface-card background. Links on hover read --foreground (--text-1).
						</Dialog.Description>
					</Dialog.Header>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story name="AllThemes" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex flex-col gap-4 p-4">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div
					data-story-theme={theme}
					data-theme={theme}
					class="rounded-lg border border-border bg-card p-4"
				>
					<p class="text-foreground mb-2 text-xs font-medium">{theme}</p>
					<Dialog.Root open>
						<Dialog.Content
							class="static inset-auto translate-x-0 translate-y-0 !transform-none shadow-none"
							portalProps={{ to: `[data-story-theme="${theme}"]` }}
						>
							<Dialog.Header>
								<Dialog.Title class="sr-only">{theme} theme preview</Dialog.Title>
								<Dialog.Description>
									Secondary text on {theme}: --muted-foreground (--text-2) on --surface-card. Links:
									<a href="#x">hover reads --foreground</a>.
								</Dialog.Description>
							</Dialog.Header>
						</Dialog.Content>
					</Dialog.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
