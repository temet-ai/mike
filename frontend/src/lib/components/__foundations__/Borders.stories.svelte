<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Foundations/Borders',
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	import { onMount } from 'svelte';

	let resolvedHex = $state<Record<string, string>>({});

	onMount(() => {
		const cs = getComputedStyle(document.documentElement);
		const tokens = ['--border-default', '--border-subtle'];
		resolvedHex = Object.fromEntries(tokens.map((t) => [t, cs.getPropertyValue(t).trim()]));
	});
</script>

<Story name="BorderColor">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Border Color Tokens</p>

			<div class="flex items-center gap-4">
				<div
					class="h-20 w-48 rounded bg-[var(--surface-card)] border border-[var(--border-default)] flex items-center justify-center"
					role="img"
					aria-label="border-default example card"
				>
					<span class="text-xs text-[var(--text-2)]">Card with border</span>
				</div>
				<div>
					<p class="text-sm text-[var(--text-strong)]">--border-default</p>
					<p class="text-xs text-[var(--text-2)]">{resolvedHex['--border-default'] || '…'}</p>
					<p class="text-caption text-[var(--text-3)]">
						All cards, inputs, table borders, separators
					</p>
					<code class="text-caption text-[var(--text-3)]">border-[var(--border-default)]</code>
				</div>
			</div>

			<div class="flex items-center gap-4">
				<div
					class="h-20 w-48 bg-[var(--surface-card)] flex flex-col overflow-hidden rounded"
					style="border: 1px solid var(--border-subtle);"
					role="img"
					aria-label="border-subtle example card"
				>
					<div
						class="px-3 py-2 border-b text-xs text-[var(--text-3)]"
						style="border-color: var(--border-subtle);"
					>
						Column header
					</div>
					<div class="flex-1 px-3 py-2 text-xs text-[var(--text-2)]">Body content</div>
				</div>
				<div>
					<p class="text-sm text-[var(--text-strong)]">--border-subtle</p>
					<p class="text-xs text-[var(--text-2)]">{resolvedHex['--border-subtle'] || '…'}</p>
					<p class="text-caption text-[var(--text-3)]">
						Hairline dividers, column header bottom borders
					</p>
					<code class="text-caption text-[var(--text-3)]">border-[var(--border-subtle)]</code>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="BorderRadius">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Border Radius Values</p>
			<p class="text-caption text-[var(--text-3)]">source: frontend/src/app.css:46 / 265 / 277</p>

			<div class="flex flex-wrap gap-6">
				<div class="flex flex-col items-center gap-2">
					<div
						class="h-10 w-24 bg-[var(--surface-2)] border border-[var(--border-default)]"
						style="border-radius: 4px;"
						role="img"
						aria-label="4px radius example"
					></div>
					<p class="text-xs text-[var(--text-strong)]">4px</p>
					<p class="text-caption text-[var(--text-3)]">rounded — small chips (.class-chip)</p>
				</div>

				<div class="flex flex-col items-center gap-2">
					<div
						class="h-10 w-24 bg-[var(--surface-2)] border border-[var(--border-default)]"
						style="border-radius: 6px;"
						role="img"
						aria-label="6px radius example"
					></div>
					<p class="text-xs text-[var(--text-strong)]">6px</p>
					<p class="text-caption text-[var(--text-3)]">rounded-md — inputs, small chips</p>
				</div>

				<div class="flex flex-col items-center gap-2">
					<div
						class="h-10 w-24 bg-[var(--surface-2)] border border-[var(--border-default)]"
						style="border-radius: 8px;"
						role="img"
						aria-label="8px radius example"
					></div>
					<p class="text-xs text-[var(--text-strong)]">8px (--radius)</p>
					<p class="text-caption text-[var(--text-3)]">rounded-lg — cards (.card-surface)</p>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Shadows">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				Shadows — Tier 1 Only on Simple Cards
			</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			<div class="flex flex-col gap-2">
				<p class="text-xs text-[var(--text-2)]">
					Tier 1 — .card-surface (inset top highlight, ONLY allowed shadow for simple cards)
				</p>
				<div class="card-surface p-4">
					<p class="text-sm text-[var(--text-1)]">Simple card using .card-surface</p>
					<p class="text-xs text-[var(--text-2)] mt-1">
						box-shadow: inset 0 1px 0 rgba(255,255,255,.055)
					</p>
				</div>
				<code class="text-caption text-[var(--text-3)]"
					>class="card-surface" — CSS class, not Tailwind utility</code
				>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs text-[var(--text-2)]">
					Tier 2 — structural turn-stream container (intentional exception only)
				</p>
				<div
					class="rounded-lg bg-[var(--surface-card)] border border-[var(--border-default)] p-4"
					style="box-shadow: inset 0 1px 0 rgba(255,255,255,0.055), 0 2px 4px -1px rgba(0,0,0,0.35), 0 4px 10px -2px rgba(0,0,0,0.28);"
				>
					<p class="text-sm text-[var(--text-1)]">Turn-stream card (.reminders / .you)</p>
					<p class="text-xs text-[var(--text-2)] mt-1">
						Tier 2 applies ONLY to .reminders and .you turn-stream containers.
					</p>
				</div>
				<p class="text-caption text-[var(--text-3)]"></p>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="WrongRight">
	{#snippet template()}
		<div class="flex flex-col gap-8 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				Common Border &amp; Shadow Mistakes
			</p>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — drop shadow on simple card (shadow-md / shadow-lg banned)
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;div class="card-surface shadow-md"&gt; &lt;!-- shadow-md is BANNED --&gt; &lt;div
						class="card-surface shadow-lg"&gt; &lt;!-- shadow-lg is BANNED --&gt;
					</code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">
					RIGHT — .card-surface only
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;div class="card-surface"&gt; &lt;!-- inset highlight is the ONLY allowed shadow
						--&gt;
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]">
					Why: drop shadows add unwanted depth hierarchy. The inset top highlight in .card-surface
					is sufficient for card elevation. source: STYLE_GUIDE.md:69 / common-mistakes.md:118-141
				</p>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — glass morphism (banned)
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;div class="backdrop-blur-sm bg-[var(--surface-card)]/80"&gt; &lt;!-- BANNED --&gt;
					</code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">RIGHT — opaque surface</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;div class="bg-[var(--surface-card)]"&gt; &lt;!-- fully opaque --&gt;
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]">
					Why: semi-transparent blurred overlays create visual noise. Content behind must be fully
					occluded. source: STYLE_GUIDE.md:72 / common-mistakes.md:145-160
				</p>
			</div>
		</div>
	{/snippet}
</Story>
