<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Foundations/Brand',
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	import { onMount } from 'svelte';

	let resolvedHex = $state<Record<string, string>>({});

	onMount(() => {
		const cs = getComputedStyle(document.documentElement);
		const tokens = [
			'--status-success-fg',
			'--interactive-active-fg',
			'--status-danger-fg',
			'--status-info-fg',
			'--data-teal-fg',
			'--text-strong'
		];
		resolvedHex = Object.fromEntries(tokens.map((t) => [t, cs.getPropertyValue(t).trim()]));
	});
</script>

<Story name="OneRule">
	{#snippet template()}
		<div
			class="flex flex-col items-center justify-center gap-8 p-16 bg-[var(--surface-bg)]"
			style="min-height: 320px;"
		>
			<blockquote class="max-w-2xl text-center">
				<p
					class="text-[var(--text-strong)]"
					style="font-family: 'Syne', sans-serif; font-size: clamp(18px, 3vw, 28px); font-weight: 700; letter-spacing: -0.03em; line-height: 1.2;"
				>
					"Every visual decision serves the data. The interface recedes so the intelligence comes
					forward."
				</p>
			</blockquote>

			<div class="flex flex-col items-center gap-2">
				<p class="text-xs text-[var(--text-3)]">This means: dark · muted · monospace · precise</p>
				<p class="text-caption text-[var(--text-faint)]"></p>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="BrandPalette">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				Brand Palette — accent tokens, all muted
			</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			<div class="flex gap-3">
				{#each [{ name: 'green (success)', token: '--status-success-fg', role: 'success / active' }, { name: 'amber (interactive)', token: '--interactive-active-fg', role: 'CTA / warning / nav cue' }, { name: 'red (danger)', token: '--status-danger-fg', role: 'error / destructive' }, { name: 'blue (info)', token: '--status-info-fg', role: 'info / focus ring' }, { name: 'teal (data)', token: '--data-teal-fg', role: 'tertiary accent' }, { name: 'text-strong', token: '--text-strong', role: 'max emphasis text' }] as color}
					<div class="flex flex-col items-center gap-2">
						<div
							class="h-14 w-14 rounded"
							style:background="var({color.token})"
							role="img"
							aria-label="{color.name} palette swatch"
						></div>
						<p class="text-caption text-[var(--text-3)] text-center">{color.name}</p>
						<p class="text-micro text-[var(--text-faint)] text-center">
							{resolvedHex[color.token] || '…'}
						</p>
					</div>
				{/each}
			</div>

			<p class="text-xs text-[var(--text-2)] max-w-xl">
				These tokens cover the core use cases. Legibility is the only constraint — each theme
				defines its own values for all accent roles.
			</p>
		</div>
	{/snippet}
</Story>

<Story name="BannedPatterns">
	{#snippet template()}
		<div class="flex flex-col gap-4 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Banned Patterns</p>
			<p class="text-caption text-[var(--text-3)]"></p>

			<div class="card-surface overflow-hidden">
				<table class="w-full text-xs">
					<thead>
						<tr class="border-b border-[var(--border-subtle)]">
							<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)] px-4 py-2"
								>Banned</th
							>
							<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)] px-4 py-2"
								>Why</th
							>
							<th class="text-left text-label uppercase tracking-kpi text-[var(--text-3)] px-4 py-2"
								>Use instead</th
							>
						</tr>
					</thead>
					<tbody>
						{#each [{ banned: 'text-white', why: 'Raw white not in system', use: 'text-[var(--text-1)] or text-[var(--text-strong)]' }, { banned: 'bg-gray-* / bg-zinc-*', why: 'Bypasses token system', use: 'bg-[var(--surface-card)] / bg-[var(--surface-bg)]' }, { banned: 'shadow-md / shadow-lg', why: 'No drop shadows in system', use: 'card-surface (inset highlight only)' }, { banned: 'dark: prefixed classes', why: 'App is dark-only', use: 'Tokens are dark values by default' }, { banned: 'backdrop-blur', why: 'Glass morphism prohibited', use: 'Opaque bg-[var(--surface-card)]' }, { banned: 'text-[#xxx] / bg-[#xxx]', why: 'Arbitrary hex bypasses tokens', use: 'Named token variable' }, { banned: 'text-[9px] / text-[10px] / text-[11px]', why: 'Bypasses type scale', use: 'text-micro / text-caption / text-label' }, { banned: 'text-[--amber] / bg-[--surface]', why: 'Tailwind v4: emits invalid CSS', use: 'text-[var(--interactive-active-fg)] / bg-[var(--surface-card)]' }, { banned: 'style:color="var(--X)"', why: 'Mixes style directives with Tailwind', use: 'class="text-[var(--X)]" (static)' }, { banned: 'Active tab: fill + text + underline', why: '3 stacked signals = flat color block', use: 'Underline dominant + text only (no fill)' }] as row}
							<tr class="border-b border-[var(--border-subtle)]">
								<td class="px-4 py-2.5">
									<code class="text-[var(--status-danger-fg)]">{row.banned}</code>
								</td>
								<td class="px-4 py-2.5 text-[var(--text-2)]">{row.why}</td>
								<td class="px-4 py-2.5">
									<code class="text-[var(--status-success-fg)]">{row.use}</code>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Composition">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				Right Composition Examples — dark · muted · monospace · precise
			</p>

			<div class="flex flex-col gap-2">
				<p class="text-xs text-[var(--text-3)]">Data row with status badge</p>
				<div class="card-surface px-4 py-3 flex items-center justify-between">
					<div class="flex items-center gap-3">
						<span class="badge-nx badge-active">
							<span class="badge-nx-dot bg-[var(--status-success-fg)]"></span>
							Active
						</span>
						<span class="text-sm text-[var(--text-1)]">POST /v1/messages</span>
					</div>
					<div class="flex items-center gap-4">
						<span class="text-xs text-[var(--text-2)]">1,247ms</span>
						<span class="text-xs text-[var(--status-success-fg)]">200</span>
					</div>
				</div>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs text-[var(--text-3)]">KPI metric — Syne value + mono label</p>
				<div class="card-surface p-4 flex flex-col gap-1">
					<p class="text-caption uppercase tracking-kpi text-[var(--text-3)]">REQUESTS TODAY</p>
					<p
						class="text-[var(--status-success-fg)]"
						style="font-family: 'Syne', sans-serif; font-size: 30px; font-weight: 700; letter-spacing: -0.02em; line-height: 1;"
					>
						4,218
					</p>
					<p class="text-xs text-[var(--text-3)]">↑ 12% vs yesterday</p>
				</div>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs text-[var(--text-3)]">Column header — Syne title + mono count</p>
				<div
					class="flex items-center justify-between px-3.5 border-b border-[var(--border-subtle)] bg-[var(--surface-bg)]"
					style="height: 48px;"
				>
					<h3
						class="text-[var(--text-strong)] m-0"
						style="font-family: 'Syne', sans-serif; font-size: 13px; font-weight: 600; letter-spacing: -0.01em;"
					>
						Requests
					</h3>
					<span class="text-micro text-[var(--text-3)]">42</span>
				</div>
			</div>
		</div>
	{/snippet}
</Story>
