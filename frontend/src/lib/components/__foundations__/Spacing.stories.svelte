<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Foundations/Spacing',
		tags: ['autodocs']
	});

	const GAP_SCALE = [
		{ tailwind: 'gap-1', px: 4, usage: 'badge padding, badge gap' },
		{ tailwind: 'gap-2', px: 8, usage: 'compact card padding, button gaps' },
		{ tailwind: 'gap-3', px: 12, usage: 'standard internal padding' },
		{ tailwind: 'gap-4', px: 16, usage: 'standard gap between elements' },
		{ tailwind: 'gap-6', px: 24, usage: 'section separators, page gaps' },
		{ tailwind: 'gap-8', px: 32, usage: 'page padding, major sections' }
	] as const;

	const PADDING_SCALE = [
		{ tailwind: 'p-2', px: 8, usage: 'compact card padding' },
		{ tailwind: 'p-3', px: 12, usage: 'standard internal padding' },
		{ tailwind: 'p-4', px: 16, usage: 'standard card padding' },
		{ tailwind: 'p-6', px: 24, usage: 'section padding' }
	] as const;
</script>

<script lang="ts">
	import { onMount } from 'svelte';

	let resolvedHex = $state<Record<string, string>>({});

	onMount(() => {
		const cs = getComputedStyle(document.documentElement);
		const tokens = ['--surface-sidebar', '--surface-bg'];
		resolvedHex = Object.fromEntries(tokens.map((t) => [t, cs.getPropertyValue(t).trim()]));
	});
</script>

<Story name="SpacingScale">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				Spacing Scale — gap values
			</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			{#each GAP_SCALE as item}
				<div class="flex items-center gap-4">
					<div class="flex shrink-0" style:gap="{item.px}px">
						<div
							class="h-6 w-6 rounded-sm bg-[var(--interactive-active-bg)] border border-[var(--interactive-active-border)]"
						></div>
						<div
							class="h-6 w-6 rounded-sm bg-[var(--interactive-active-bg)] border border-[var(--interactive-active-border)]"
						></div>
						<div
							class="h-6 w-6 rounded-sm bg-[var(--interactive-active-bg)] border border-[var(--interactive-active-border)]"
						></div>
					</div>
					<div class="flex items-baseline gap-3">
						<code class="text-sm text-[var(--text-strong)] min-w-16">{item.tailwind}</code>
						<span class="text-xs text-[var(--text-2)] min-w-10">{item.px}px</span>
						<span class="text-xs text-[var(--text-3)]">{item.usage}</span>
					</div>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story name="PaddingScale">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				Padding Scale — p-* values
			</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			{#each PADDING_SCALE as item}
				<div class="flex items-center gap-4">
					<div class="border border-dashed border-[var(--border-default)] inline-block">
						<div
							class="bg-[var(--surface-2)] border border-[var(--border-default)]"
							style:padding="{item.px}px"
						>
							<div
								class="w-16 h-4 rounded bg-[var(--interactive-active-bg)] border border-[var(--interactive-active-border)]"
							></div>
						</div>
					</div>
					<div class="flex items-baseline gap-3">
						<code class="text-sm text-[var(--text-strong)] min-w-10">{item.tailwind}</code>
						<span class="text-xs text-[var(--text-2)] min-w-10">{item.px}px</span>
						<span class="text-xs text-[var(--text-3)]">{item.usage}</span>
					</div>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story name="GridShell">
	{#snippet template()}
		<div class="flex flex-col gap-4 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				App Shell — 4-Pane Grid (phase 2.11)
			</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			<div
				class="border border-[var(--border-default)] rounded overflow-hidden"
				style="display: grid; grid-template-rows: 44px 1fr; height: 320px;"
				role="img"
				aria-label="App shell grid sketch"
			>
				<div
					class="flex items-center px-4 border-b border-[var(--border-default)]"
					style="background: var(--surface-sidebar);"
				>
					<span class="text-xs text-[var(--text-3)]">TopBar · 44px · grid-column: 1/-1</span>
				</div>

				<div style="display: grid; grid-template-columns: 132px 260px 380px 1fr;">
					<div
						class="flex flex-col items-center justify-center border-r border-[var(--border-default)] gap-1"
						style="background: var(--surface-sidebar);"
					>
						<span class="text-micro text-[var(--text-3)] text-center">NavRail</span>
						<span class="text-micro text-[var(--text-faint)]">132px</span>
					</div>

					<div
						class="flex flex-col items-center justify-center border-r border-[var(--border-default)] gap-1 bg-[var(--surface-sidebar)]"
					>
						<span class="text-micro text-[var(--text-3)] text-center">Sessions</span>
						<span class="text-micro text-[var(--text-faint)]">260px</span>
					</div>

					<div
						class="flex flex-col items-center justify-center border-r border-[var(--border-default)] gap-1 bg-[var(--surface-card)]"
					>
						<span class="text-micro text-[var(--text-3)] text-center">Requests</span>
						<span class="text-micro text-[var(--text-faint)]">380px</span>
					</div>

					<div class="flex flex-col items-center justify-center gap-1 bg-[var(--surface-bg)]">
						<span class="text-micro text-[var(--text-3)] text-center">Detail</span>
						<span class="text-micro text-[var(--text-faint)]">1fr</span>
					</div>
				</div>
			</div>

			<div class="card-surface p-4">
				<table class="w-full text-xs">
					<thead>
						<tr class="border-b border-[var(--border-subtle)]">
							<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)] pb-2"
								>Column</th
							>
							<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)] pb-2"
								>Width</th
							>
							<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)] pb-2"
								>Background</th
							>
						</tr>
					</thead>
					<tbody class="text-[var(--text-2)]">
						<tr class="border-b border-[var(--border-subtle)]">
							<td class="py-1.5">.rail (NavRail)</td>
							<td class="py-1.5">132px</td>
							<td class="py-1.5 font-mono"
								>--surface-sidebar ({resolvedHex['--surface-sidebar'] || '…'})</td
							>
						</tr>
						<tr class="border-b border-[var(--border-subtle)]">
							<td class="py-1.5">.col.sessions</td>
							<td class="py-1.5">260px</td>
							<td class="py-1.5 font-mono">--surface-sidebar</td>
						</tr>
						<tr class="border-b border-[var(--border-subtle)]">
							<td class="py-1.5">.col.requests</td>
							<td class="py-1.5">380px</td>
							<td class="py-1.5 font-mono">--surface-card</td>
						</tr>
						<tr>
							<td class="py-1.5">.col.detail</td>
							<td class="py-1.5">1fr</td>
							<td class="py-1.5 font-mono">--surface-bg ({resolvedHex['--surface-bg'] || '…'})</td>
						</tr>
					</tbody>
				</table>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="WrongRight">
	{#snippet template()}
		<div class="flex flex-col gap-8 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Common Spacing Mistakes</p>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — arbitrary px bypasses scale
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;div class="p-[15px] gap-[14px]"&gt;
					</code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">
					RIGHT — scale-aligned utilities
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;div class="p-4 gap-3"&gt; &lt;!-- p-4=16px gap-3=12px --&gt;
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]">
					Why: scale steps create visual rhythm. Arbitrary values create inconsistency.
				</p>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — old sidebar+content layout (deleted in phase 2.11)
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;aside class="w-[280px]"&gt;...&lt;/aside&gt; &lt;main
						class="flex-1"&gt;...&lt;/main&gt;
					</code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">RIGHT — 4-pane CSS grid</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;div class="shell"&gt; &lt;!-- grid-template-rows: 44px 1fr --&gt; &lt;TopBar /&gt;
						&lt;div class="app"&gt; &lt;!-- grid-template-columns: 132px 260px 380px 1fr --&gt;
						&lt;NavRail /&gt; &lt;SessionsColumn /&gt; &lt;RequestsColumn /&gt;
						&lt;RequestDetailColumn /&gt; &lt;/div&gt; &lt;/div&gt;
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]">
					Why: sidebar+main pattern was superseded in phase 2.11.
				</p>
			</div>
		</div>
	{/snippet}
</Story>
