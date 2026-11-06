<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Dialog from './index';

	let boundHeaderEl: HTMLDivElement | null = null;

	const { Story } = defineMeta({
		title: 'UI/Dialog/DialogHeader',
		component: Dialog.Header,
		tags: ['autodocs'],
		argTypes: {
			class: { control: 'text' }
		}
	});
</script>

<Story name="Empty">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg" aria-label="Empty header demo">
				<Dialog.Header />
				<p class="text-muted-foreground text-sm mt-4">
					(Header rendered above with no children — data-slot div is present)
				</p>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithTitle">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Confirm deletion</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithDescription">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content
				showCloseButton={false}
				class="sm:max-w-lg"
				aria-label="Description-only dialog"
			>
				<Dialog.Header>
					<Dialog.Description>
						Description only, no title. The header renders the flex container around the description
						without requiring a sibling title element.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithTitleAndDescription">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Delete session data</Dialog.Title>
					<Dialog.Description>
						This action cannot be undone. The session and all captured requests will be permanently
						removed from the proxy store.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithCustomContent">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content
				showCloseButton={false}
				class="sm:max-w-lg"
				aria-label="Custom content dialog"
			>
				<Dialog.Header>
					<p class="text-foreground font-semibold text-base">Custom heading paragraph</p>
					<p class="text-muted-foreground text-sm">
						Custom snippet content — not Dialog.Title or Dialog.Description, just arbitrary markup
						passed as children.
					</p>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ClassOverride"
	tags={['visual']}
	args={{ class: 'border border-[var(--destructive)] rounded p-2' }}
	play={async ({ canvasElement: _canvas }) => {
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		if (content) {
			await captureStorySnapshot(content, 'dialog-header/ClassOverride');
		}
	}}
>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header {...args}>
					<Dialog.Title>Custom class merge</Dialog.Title>
					<Dialog.Description>
						The red border is injected via the class prop. Both the border-* custom class and the
						default flex flex-col gap-2 must coexist after cn() merge.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="WithFooter"
	tags={['visual']}
	play={async ({ canvasElement: _canvas }) => {
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		if (content) {
			await captureStorySnapshot(content, 'dialog-header/WithFooter');
		}
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Confirm action</Dialog.Title>
					<Dialog.Description>
						Header composed above footer. flex-col ordering in Dialog.Content ensures header renders
						first, footer renders last.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer showCloseButton={true} />
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveDataSlot"
	tags={['interactive']}
	play={async () => {
		const header = document.body.querySelector('[data-slot="dialog-header"]');
		await expect(
			header,
			'dialog-header element with data-slot="dialog-header" must exist'
		).toBeTruthy();

		await expect(header?.classList.contains('flex'), 'flex class must be present').toBe(true);
		await expect(header?.classList.contains('flex-col'), 'flex-col class must be present').toBe(
			true
		);
		await expect(header?.classList.contains('gap-2'), 'gap-2 class must be present').toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Data-slot assertion</Dialog.Title>
					<Dialog.Description>
						The play function checks data-slot="dialog-header" and the flex flex-col gap-2 classes.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveClassMerge"
	tags={['interactive']}
	play={async () => {
		const header = document.body.querySelector('[data-slot="dialog-header"]');
		await expect(header, 'dialog-header must be in DOM').toBeTruthy();

		await expect(
			header?.classList.contains('flex'),
			'default flex class must survive cn() merge'
		).toBe(true);
		await expect(
			header?.classList.contains('flex-col'),
			'default flex-col class must survive cn() merge'
		).toBe(true);

		await expect(
			header?.classList.contains('rounded'),
			'injected rounded class must be present after cn() merge'
		).toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header class="rounded p-2">
					<Dialog.Title>Class merge assertion</Dialog.Title>
					<Dialog.Description>
						Both the custom rounded class and the default flex classes must coexist.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveTitleAndDescription"
	tags={['interactive']}
	play={async () => {
		const header = document.body.querySelector('[data-slot="dialog-header"]');
		await expect(header, 'dialog-header must be in DOM').toBeTruthy();

		const title = header?.querySelector('[data-slot="dialog-title"]');
		await expect(title, 'dialog-title must be a child of dialog-header').toBeTruthy();
		await expect(title?.textContent?.trim()).toBe('Interactive title');

		const desc = header?.querySelector('[data-slot="dialog-description"]');
		await expect(desc, 'dialog-description must be a child of dialog-header').toBeTruthy();
		await expect(desc?.textContent?.trim()).toBe('Interactive description text.');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Interactive title</Dialog.Title>
					<Dialog.Description>Interactive description text.</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	play={async () => {
		const domHeader = document.body.querySelector<HTMLDivElement>(
			'[data-slot="dialog-header"][data-testid="ref-binding-header"]'
		);
		await expect(
			domHeader,
			'dialog-header with data-testid="ref-binding-header" must be in DOM after mount'
		).toBeTruthy();

		await expect(
			boundHeaderEl,
			'boundHeaderEl must be non-null after mount (bind:ref wiring)'
		).toBeTruthy();
		await expect(
			boundHeaderEl instanceof HTMLDivElement,
			'ref must be an HTMLDivElement — Dialog.Header renders a <div>'
		).toBe(true);
		await expect(
			boundHeaderEl?.isSameNode(domHeader),
			'boundHeaderEl must point to the same DOM node as the live document.body query — stale refs from prior runs will fail this check'
		).toBe(true);
		await expect(
			boundHeaderEl?.getAttribute('data-slot'),
			'bound element must carry data-slot="dialog-header"'
		).toBe('dialog-header');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header bind:ref={boundHeaderEl} data-testid="ref-binding-header">
					<Dialog.Title>Ref binding dialog</Dialog.Title>
					<Dialog.Description>
						bind:ref writes the underlying HTMLDivElement into boundHeaderEl. The play function
						cross-checks boundHeaderEl against the live DOM element via isSameNode — a stale ref
						from a prior run will not match the current node, making the $bindable() ref contract
						verifiable across re-runs.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveRestProps"
	tags={['interactive']}
	play={async () => {
		const header = document.body.querySelector('[data-slot="dialog-header"]');
		await expect(header, 'dialog-header must be in DOM').toBeTruthy();

		await expect(
			header?.getAttribute('data-testid'),
			'data-testid from restProps must be forwarded to the div element'
		).toBe('test-header');

		await expect(
			header?.getAttribute('aria-label'),
			'aria-label from restProps must be forwarded to the div element'
		).toBe('dialog heading region');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header data-testid="test-header" aria-label="dialog heading region">
					<Dialog.Title>RestProps dialog</Dialog.Title>
					<Dialog.Description>
						data-testid and aria-label are passed via restProps spread to the underlying div.
					</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="SnapshotDefault"
	tags={['visual']}
	play={async ({ canvasElement: _canvas }) => {
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		if (content) {
			await captureStorySnapshot(content, 'dialog-header/Default');
		}
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
	play={async ({ canvasElement: _canvas }) => {
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		if (content) {
			await captureStorySnapshot(content, 'dialog-header/ThemeForge');
		}
	}}
>
	{#snippet template()}
		<div data-theme="forge">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Forge theme</Dialog.Title>
						<Dialog.Description>
							Title reads --text-1 and description reads --text-2 on the --surface-card background
							in the forge (default dark) theme.
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
	play={async ({ canvasElement: _canvas }) => {
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		if (content) {
			await captureStorySnapshot(content, 'dialog-header/ThemeGlacier');
		}
	}}
>
	{#snippet template()}
		<div data-theme="glacier">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Glacier theme</Dialog.Title>
						<Dialog.Description>
							Title reads --text-1 and description reads --text-2 on the --surface-card background
							in the glacier (cool blue-grey) theme.
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
	play={async ({ canvasElement: _canvas }) => {
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		if (content) {
			await captureStorySnapshot(content, 'dialog-header/ThemeVoid');
		}
	}}
>
	{#snippet template()}
		<div data-theme="void">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Void theme</Dialog.Title>
						<Dialog.Description>
							Title reads --text-1 and description reads --text-2 on the near-black --surface-card
							background in the void theme (violet-tinted text).
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
	play={async ({ canvasElement: _canvas }) => {
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		if (content) {
			await captureStorySnapshot(content, 'dialog-header/ThemePhosphor');
		}
	}}
>
	{#snippet template()}
		<div data-theme="phosphor">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Phosphor theme</Dialog.Title>
						<Dialog.Description>
							Title reads --text-1 and description reads --text-2 on the near-pure-black surface in
							the phosphor theme (CRT green terminal aesthetic).
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
	play={async ({ canvasElement: _canvas }) => {
		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		if (content) {
			await captureStorySnapshot(content, 'dialog-header/ThemeCrimson');
		}
	}}
>
	{#snippet template()}
		<div data-theme="crimson">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Crimson theme</Dialog.Title>
						<Dialog.Description>
							Title reads --text-1 and description reads --text-2 on the warm-dark brown
							--surface-card background in the crimson theme.
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
	play={async ({ canvasElement: _canvas }) => {
		await captureStorySnapshot(document.body, 'dialog-header/AllThemes');
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
									Header on {theme}: title reads --text-1; description reads --text-2 on
									--surface-card.
								</Dialog.Description>
							</Dialog.Header>
						</Dialog.Content>
					</Dialog.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
