<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import * as Popover from './index';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Popover',
		component: Popover.Root,
		tags: ['autodocs']
	});
</script>

<Story name="Default">
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open popover</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">
						Popover content renders here when the trigger is activated.
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

<Story name="Open">
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

<Story name="WithForm">
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
								<label for="popover-rule-kind" class="text-[var(--text-2)] text-xs">Rule kind</label
								>
								<select
									id="popover-rule-kind"
									class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
								>
									<option value="text_substring">Text substring</option>
									<option value="xml_tag">XML tag</option>
								</select>
							</div>
							<div class="space-y-1">
								<label for="popover-rule-value" class="text-[var(--text-2)] text-xs"
									>Match value</label
								>
								<input
									id="popover-rule-value"
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

<Story name="Anchored">
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
