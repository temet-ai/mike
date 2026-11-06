<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import * as Sheet from './index';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Sheet',
		component: Sheet.Root,
		tags: ['autodocs']
	});
</script>

<Story name="RightSheet">
	{#snippet template()}
		<div class="relative h-[500px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Help & references</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							This drawer slides in from the right. It occupies a fixed 420px width and full
							viewport height, anchored to the trailing edge of the screen.
						</p>
						<p class="text-[var(--text-2)] text-xs">
							Production usage: HelpDrawer, RequestDrawer — both use this same right-anchored
							pattern. See
							<code class="font-mono">sheet-content.svelte</code> for the CSS spec.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithForm">
	{#snippet template()}
		<div class="relative h-[600px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} class="flex flex-col">
					<div class="flex flex-col gap-1 border-b border-[var(--border-subtle)] px-5 py-4">
						<Sheet.Title>Filter rules</Sheet.Title>
						<p class="text-[var(--text-2)] text-xs">
							Rules are applied in order. Each rule either drops, replaces, or appends to matching
							system-prompt blocks.
						</p>
					</div>
					<div class="flex-1 overflow-y-auto px-5 py-4">
						<form id="filter-rules-form" class="space-y-4">
							<fieldset class="space-y-3 border-0 p-0">
								<legend
									class="text-[var(--text-strong)] text-xs font-semibold uppercase tracking-wider"
								>
									New rule
								</legend>
								<div class="space-y-1">
									<label for="sheet-rule-kind" class="text-[var(--text-2)] text-xs">Rule kind</label
									>
									<select
										id="sheet-rule-kind"
										class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
									>
										<option value="text_substring">Text substring</option>
										<option value="xml_tag">XML tag</option>
									</select>
								</div>
								<div class="space-y-1">
									<label for="sheet-rule-value" class="text-[var(--text-2)] text-xs"
										>Match value</label
									>
									<input
										id="sheet-rule-value"
										type="text"
										placeholder="<parameter>"
										class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 font-mono text-[var(--text-1)] text-sm outline-none placeholder:text-[var(--text-3)] focus:ring-1 focus:ring-[var(--status-info-fg)]"
									/>
								</div>
								<div class="space-y-1">
									<label for="sheet-rule-action" class="text-[var(--text-2)] text-xs">Action</label>
									<select
										id="sheet-rule-action"
										class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
									>
										<option value="drop">Drop</option>
										<option value="replace">Replace</option>
										<option value="append">Append</option>
									</select>
								</div>
							</fieldset>
						</form>
					</div>
					<div class="flex justify-end gap-2 border-t border-[var(--border-subtle)] px-5 py-3">
						<Button variant="ghost" size="sm">Cancel</Button>
						<Button variant="default" size="sm" form="filter-rules-form" type="submit">
							Save rule
						</Button>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithDescription">
	{#snippet template()}
		<div class="relative h-[500px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<div class="space-y-1">
							<Sheet.Title>Request details</Sheet.Title>
							<Sheet.Description>
								Full request body, headers, and response for request
								<code class="font-mono text-[var(--text-1)]">req_01JT4X…</code>
							</Sheet.Description>
						</div>
						<div
							class="rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] p-3 font-mono text-[var(--text-1)] text-xs"
						>
							<p class="text-[var(--text-2)]">// request body preview</p>
							<p class="mt-1">{`{ "model": "claude-opus-4-5", "max_tokens": 8192 }`}</p>
						</div>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithTrigger">
	{#snippet template()}
		<div class="flex h-[400px] items-center justify-center bg-[var(--surface-bg)]">
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
							This sheet was opened by clicking the trigger button in the main content area. The
							close button (×) in the top-right corner dismisses it.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>
