<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import { THEMES as THEME_REGISTRY } from '$lib/themes';

	const { Story } = defineMeta({
		title: 'Foundations/Themes',
		tags: ['autodocs']
	});

	const THEMES = [
		{
			id: 'forge',
			label: 'FORGE',
			desc: 'FUNK · cyan navigation · magenta actions · production theme',
			accentToken: '--interactive-active-fg',
			identityToken: '--interactive-active-fg'
		},
		{
			id: 'glacier',
			label: 'GLACIER',
			desc: 'Arctic blue · clinical precision · high-contrast cool grey',
			accentToken: '--glacier',
			identityToken: '--glacier'
		},
		{
			id: 'void',
			label: 'VOID',
			desc: 'Electric violet · near-black surfaces · hacker aesthetic',
			accentToken: '--volt',
			identityToken: '--volt'
		},
		{
			id: 'phosphor',
			label: 'PHOSPHOR',
			desc: 'CRT green · pure black · 1980s mainframe terminal',
			accentToken: '--phosphor',
			identityToken: '--phosphor'
		},
		{
			id: 'crimson',
			label: 'CRIMSON',
			desc: 'Blood red · editorial gold · intelligence-gathering drama',
			accentToken: '--interactive-active-fg',
			identityToken: '--crimson'
		}
	];

	const SURFACE_TOKENS = ['--surface-sidebar', '--surface-bg', '--surface-card', '--surface-2'];
	const BORDER_TOKENS = ['--border-default', '--border-subtle', '--border-strong'];
	const TEXT_TOKENS = ['--text-strong', '--text-1', '--text-2', '--text-3', '--text-faint'];
	const ACCENT_TOKENS = [
		'--status-success-fg',
		'--interactive-active-fg',
		'--status-info-fg',
		'--status-danger-fg',
		'--status-steering-fg',
		'--data-teal-fg'
	];
	const RAMP_TOKENS = [
		'--green-bright',
		'--green-muted',
		'--amber-bright',
		'--amber-warm',
		'--blue-bright',
		'--blue-deep',
		'--red-bright',
		'--red-muted'
	];
	const EXTENDED_TOKENS = ['--notice', '--pine', '--gold'];
</script>

<script lang="ts">
	import { onMount } from 'svelte';

	let resolvedTokens = $state<Record<string, string>>({});
	let perThemeTokens = $state<Record<string, Record<string, string>>>({});

	onMount(() => {
		const allTokens = [
			...SURFACE_TOKENS,
			...BORDER_TOKENS,
			...TEXT_TOKENS,
			...ACCENT_TOKENS,
			...RAMP_TOKENS,
			...EXTENDED_TOKENS
		];

		const cs = getComputedStyle(document.documentElement);
		resolvedTokens = Object.fromEntries(allTokens.map((t) => [t, cs.getPropertyValue(t).trim()]));

		const result: Record<string, Record<string, string>> = {};
		document.querySelectorAll<HTMLElement>('[data-theme]').forEach((el) => {
			const id = el.getAttribute('data-theme');
			if (!id || id in result) return;
			const elCs = getComputedStyle(el);
			result[id] = Object.fromEntries(allTokens.map((t) => [t, elCs.getPropertyValue(t).trim()]));
		});
		perThemeTokens = result;
	});
</script>

<Story
	name="Gallery — All Themes"
	play={async ({ canvas }) => {
		for (const theme of THEME_REGISTRY) {
			const panel = canvas.getByTestId(`theme-panel-${theme.id}`);
			const direct = canvas.getByTestId(`theme-direct-${theme.id}`);
			const scoped = canvas.getByTestId(`theme-funk-${theme.id}`);
			const panelStyle = getComputedStyle(panel);
			const directStyle = getComputedStyle(direct);
			const scopedStyle = getComputedStyle(scoped);

			await expect(scopedStyle.backgroundColor).toBe(directStyle.backgroundColor);
			await expect(scopedStyle.color).toBe(directStyle.color);
			for (const token of [...SURFACE_TOKENS, ...TEXT_TOKENS, '--surface-3', '--primary']) {
				const expected = panelStyle.getPropertyValue(token).trim();
				await expect(expected).not.toBe('');
				await expect(scopedStyle.getPropertyValue(token).trim()).toBe(expected);
			}
			await expect(panelStyle.getPropertyValue('--primary').trim()).toBe(
				panelStyle.getPropertyValue('--status-steering-fg').trim()
			);
			await expect(panelStyle.getPropertyValue('--primary').trim()).not.toBe(
				panelStyle.getPropertyValue('--interactive-active-fg').trim()
			);
		}
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-8 p-6" style="background:var(--surface-sidebar)">
			<p style="color:var(--text-3);font-size:11px;letter-spacing:0.08em;text-transform:uppercase">
				Theme Gallery — 5 named themes. Each panel renders under its real [data-theme] block from
				app.css so all text, surfaces, and accents are OKLCH values.
			</p>

			{#each THEMES as theme (theme.id)}
				<div
					data-theme={theme.id}
					data-testid={`theme-panel-${theme.id}`}
					class="rounded-lg overflow-hidden"
					style="border:1px solid var(--border-default)"
				>
					<div
						class="flex items-center justify-between px-4 py-3"
						style="background:var(--surface-sidebar);border-bottom:1px solid var(--border-subtle)"
					>
						<div class="flex items-center gap-3">
							<div
								class="h-2 w-2 rounded-full"
								style="background:var({theme.accentToken});box-shadow:0 0 8px var({theme.accentToken})"
							></div>
							<span
								style="font-family:'JetBrains Mono Variable',monospace;font-size:11px;letter-spacing:0.1em;color:var(--text-strong);font-weight:600"
								>{theme.label}</span
							>
						</div>
						<span style="font-size:10px;color:var(--text-3)">{theme.desc}</span>
					</div>

					<div class="flex gap-0" style="background:var(--surface-bg)">
						{#each [{ label: '--surface-sidebar', token: '--surface-sidebar' }, { label: '--surface-bg', token: '--surface-bg' }, { label: '--surface-card', token: '--surface-card' }, { label: '--surface-2', token: '--surface-2' }] as s (s.token)}
							<div
								class="flex-1 flex flex-col items-center py-3 gap-1"
								style="background:var({s.token});border-right:1px solid var(--border-subtle)"
							>
								<span style="font-size:9px;color:var(--text-3);letter-spacing:0.06em"
									>{s.label}</span
								>
							</div>
						{/each}
					</div>

					<div
						class="flex gap-0"
						style="background:var(--surface-card);border-top:1px solid var(--border-subtle)"
					>
						{#each [{ label: 'success', token: '--status-success-fg' }, { label: 'active', token: '--interactive-active-fg' }, { label: 'info', token: '--status-info-fg' }, { label: 'danger', token: '--status-danger-fg' }, { label: 'steering', token: '--status-steering-fg' }, { label: 'teal', token: '--data-teal-fg' }] as a (a.token)}
							<div
								class="flex-1 flex flex-col items-center py-2 gap-1"
								style="border-right:1px solid var(--border-subtle)"
							>
								<div class="h-4 w-4 rounded-full" style="background:var({a.token})"></div>
								<span style="font-size:9px;color:var(--text-faint)">{a.label}</span>
							</div>
						{/each}
					</div>

					<div
						class="px-4 py-3 flex flex-col gap-1"
						style="background:var(--surface-bg);border-top:1px solid var(--border-subtle)"
					>
						<span
							style="color:var(--text-strong);font-size:13px;font-family:'JetBrains Mono Variable',monospace"
							>text-strong — body</span
						>
						<span
							style="color:var(--text-1);font-size:12px;font-family:'JetBrains Mono Variable',monospace"
							>text-1 — body default</span
						>
						<span
							style="color:var(--text-2);font-size:11px;font-family:'JetBrains Mono Variable',monospace"
							>text-2 — secondary</span
						>
						<span
							style="color:var(--text-3);font-size:10px;font-family:'JetBrains Mono Variable',monospace"
							>text-3 — muted / labels</span
						>
					</div>

					<div
						class="grid gap-2 p-4 sm:grid-cols-2"
						style="background:var(--surface-bg);border-top:var(--hairline) solid var(--border-subtle)"
					>
						<div
							data-testid={`theme-direct-${theme.id}`}
							class="p-3 text-sm"
							style="background:var(--surface-card);color:var(--text-1)"
						>
							Theme surface
						</div>
						<div
							data-design="funk"
							data-testid={`theme-funk-${theme.id}`}
							class="p-3 text-sm"
							style="background:var(--surface-1);color:var(--text-1)"
						>
							FUNK surface · inherits the theme
						</div>
					</div>

					<div
						class="flex items-center gap-2 px-4 py-3"
						style="background:var(--surface-card);border-top:1px solid var(--border-subtle)"
					>
						<span
							style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-success-bg);color:var(--status-success-fg);border:1px solid var(--status-success-border)"
							><span
								style="width:5px;height:5px;border-radius:50%;background:var(--status-success-fg)"
							></span>Active</span
						>
						<span
							style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-warning-bg);color:var(--status-warning-fg);border:1px solid var(--status-warning-border)"
							>Warning</span
						>
						<span
							style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-danger-bg);color:var(--status-danger-fg);border:1px solid var(--status-danger-border)"
							>Error</span
						>
						<span
							style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-info-bg);color:var(--status-info-fg);border:1px solid var(--status-info-border)"
							>Analyzed</span
						>
						<span
							style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-muted-bg);color:var(--text-3);border:1px solid var(--status-muted-border)"
							>Muted</span
						>

						<div
							style="margin-left:auto;padding:5px 14px;border-radius:var(--r-tol);font-size:var(--text-xs);font-family:var(--font-machine);background:var(--primary);color:var(--primary-foreground);font-weight:600;cursor:default"
						>
							Primary CTA
						</div>
					</div>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story name="Forge — Token Reference">
	{#snippet template()}
		<div data-theme="forge" class="p-6 flex flex-col gap-6" style="background:var(--surface-bg)">
			{@render ThemeHeader(
				'FORGE',
				'Volcanic amber · iron-dark · production theme',
				'--interactive-active-fg'
			)}
			{@render TokenGrid('forge')}
			{@render ComponentSamples()}
		</div>
	{/snippet}
</Story>

<Story name="Glacier — Token Reference">
	{#snippet template()}
		<div data-theme="glacier" class="p-6 flex flex-col gap-6" style="background:var(--surface-bg)">
			{@render ThemeHeader(
				'GLACIER',
				'Arctic blue · clinical precision · high-contrast cool grey',
				'--glacier'
			)}
			<div
				class="p-4 rounded"
				style="background:var(--surface-card);border:1px solid var(--border-default)"
			>
				<p
					style="color:var(--text-3);font-size:10px;letter-spacing:0.06em;margin-bottom:8px;text-transform:uppercase"
				>
					Theme-specific identity token
				</p>
				<div class="flex items-center gap-3">
					<div class="h-8 w-8 rounded" style="background:var(--glacier)"></div>
					<div>
						<p
							style="color:var(--text-strong);font-size:12px;font-family:'JetBrains Mono Variable',monospace"
						>
							--glacier — arctic blue H=203
						</p>
						<p style="color:var(--text-2);font-size:11px">
							Primary accent, interactive-active-fg maps here.
						</p>
					</div>
				</div>
			</div>
			{@render TokenGrid('glacier')}
			{@render ComponentSamples()}
		</div>
	{/snippet}
</Story>

<Story name="Void — Token Reference">
	{#snippet template()}
		<div data-theme="void" class="p-6 flex flex-col gap-6" style="background:var(--surface-bg)">
			{@render ThemeHeader(
				'VOID',
				'Electric violet · near-black surfaces · hacker aesthetic',
				'--volt'
			)}
			<div
				class="p-4 rounded"
				style="background:var(--surface-card);border:1px solid var(--border-default)"
			>
				<p
					style="color:var(--text-3);font-size:10px;letter-spacing:0.06em;margin-bottom:8px;text-transform:uppercase"
				>
					Theme-specific tokens (volt family)
				</p>
				<div class="flex flex-col gap-3">
					{#each [{ label: '--volt', desc: 'Electric violet — primary accent' }, { label: '--volt-dim', desc: 'Tint background' }, { label: '--volt-border', desc: 'Ring / border' }, { label: '--volt-glow', desc: 'Glow shadow' }] as t (t.label)}
						<div class="flex items-center gap-3">
							<div class="h-6 w-6 rounded flex-shrink-0" style="background:var({t.label})"></div>
							<span
								style="color:var(--text-strong);font-size:11px;font-family:'JetBrains Mono Variable',monospace"
								>{t.label}</span
							>
							<span style="color:var(--text-2);font-size:11px">{t.desc}</span>
							<span
								style="color:var(--text-3);font-size:9px;margin-left:auto;font-family:'JetBrains Mono Variable',monospace"
								>{resolvedTokens[t.label] ?? '…'}</span
							>
						</div>
					{/each}
				</div>
			</div>
			{@render TokenGrid('void')}
			<div
				class="p-4 rounded"
				style="background:var(--surface-card);border:1px solid var(--border-default)"
			>
				<p
					style="color:var(--text-3);font-size:10px;letter-spacing:0.06em;margin-bottom:10px;text-transform:uppercase"
				>
					Volt accent in action
				</p>
				<div class="flex items-center gap-3 flex-wrap">
					<div
						style="padding:3px 10px;border-radius:4px;font-size:11px;background:var(--volt-dim);color:var(--volt);border:1px solid var(--volt-border);font-family:'JetBrains Mono Variable',monospace"
					>
						VOLT CHIP
					</div>
					<div
						style="padding:3px 10px;border-radius:6px;font-size:11px;background:var(--volt);color:var(--surface-sidebar);font-family:'JetBrains Mono Variable',monospace;font-weight:600"
					>
						Primary CTA
					</div>
					<div
						style="padding:8px 10px;border-radius:6px;font-size:11px;background:var(--surface-2);border:1px solid var(--volt-border);font-family:'JetBrains Mono Variable',monospace;color:var(--text-1)"
					>
						Focus ring demo
					</div>
				</div>
			</div>
			{@render ComponentSamples()}
		</div>
	{/snippet}
</Story>

<Story name="Phosphor — Token Reference">
	{#snippet template()}
		<div data-theme="phosphor" class="p-6 flex flex-col gap-6" style="background:var(--surface-bg)">
			{@render ThemeHeader(
				'PHOSPHOR',
				'CRT green · pure black · 1980s mainframe terminal',
				'--phosphor'
			)}
			<div
				class="p-4 rounded"
				style="background:var(--surface-card);border:1px solid var(--border-default)"
			>
				<p
					style="color:var(--text-3);font-size:10px;letter-spacing:0.06em;margin-bottom:8px;text-transform:uppercase"
				>
					Theme-specific tokens (phosphor family)
				</p>
				<div class="flex flex-col gap-3">
					{#each [{ label: '--phosphor', desc: 'Classic P1 phosphor green — primary accent' }, { label: '--phosphor-dim', desc: 'Tint background' }, { label: '--phosphor-border', desc: 'Ring / border (higher alpha for legibility on near-black)' }, { label: '--phosphor-glow', desc: 'CRT phosphor glow shadow' }] as t (t.label)}
						<div class="flex items-center gap-3">
							<div class="h-6 w-6 rounded flex-shrink-0" style="background:var({t.label})"></div>
							<span
								style="color:var(--text-strong);font-size:11px;font-family:'JetBrains Mono Variable',monospace"
								>{t.label}</span
							>
							<span style="color:var(--text-2);font-size:11px">{t.desc}</span>
						</div>
					{/each}
				</div>
			</div>
			{@render TokenGrid('phosphor')}
			<div
				class="p-4 rounded"
				style="background:var(--surface-bg);border:1px solid var(--border-default)"
			>
				<p
					style="color:var(--text-3);font-size:10px;letter-spacing:0.06em;margin-bottom:10px;text-transform:uppercase"
				>
					CRT character aesthetic
				</p>
				<div class="flex flex-col gap-2">
					<p
						style="color:var(--phosphor);font-family:'JetBrains Mono Variable',monospace;font-size:13px;text-shadow:0 0 8px var(--phosphor-glow)"
					>
						mike@inspector:~$ list-sessions
					</p>
					<p
						style="color:var(--text-1);font-family:'JetBrains Mono Variable',monospace;font-size:12px"
					>
						SESSION 2026-06-16T14:23:01Z 8 requests 4.2s avg
					</p>
					<p
						style="color:var(--text-2);font-family:'JetBrains Mono Variable',monospace;font-size:12px"
					>
						SESSION 2026-06-16T14:15:44Z 12 requests 6.1s avg
					</p>
					<p
						style="color:var(--status-success-fg);font-family:'JetBrains Mono Variable',monospace;font-size:12px;text-shadow:0 0 6px var(--phosphor-glow)"
					>
						2 sessions active · 20 total requests
					</p>
				</div>
			</div>
			{@render ComponentSamples()}
		</div>
	{/snippet}
</Story>

<Story name="Crimson — Token Reference">
	{#snippet template()}
		<div data-theme="crimson" class="p-6 flex flex-col gap-6" style="background:var(--surface-bg)">
			{@render ThemeHeader(
				'CRIMSON',
				'Blood red · editorial gold · intelligence-gathering drama',
				'--interactive-active-fg'
			)}
			<div
				class="p-4 rounded"
				style="background:var(--surface-card);border:1px solid var(--border-default)"
			>
				<p
					style="color:var(--text-3);font-size:10px;letter-spacing:0.06em;margin-bottom:8px;text-transform:uppercase"
				>
					Theme-specific identity tokens
				</p>
				<div class="flex flex-col gap-3">
					{#each [{ label: '--crimson', desc: 'Deep blood red — interactive-active-fg' }, { label: '--crimson-dim', desc: 'Crimson tint background' }, { label: '--crimson-border', desc: 'Crimson ring / border' }, { label: '--gold', desc: 'Editorial gold — warning, role-tool, secondary highlight' }, { label: '--gold-dim', desc: 'Gold tint background' }, { label: '--gold-border', desc: 'Gold ring / border' }] as t (t.label)}
						<div class="flex items-center gap-3">
							<div class="h-6 w-6 rounded flex-shrink-0" style="background:var({t.label})"></div>
							<span
								style="color:var(--text-strong);font-size:11px;font-family:'JetBrains Mono Variable',monospace"
								>{t.label}</span
							>
							<span style="color:var(--text-2);font-size:11px">{t.desc}</span>
						</div>
					{/each}
				</div>
			</div>
			{@render TokenGrid('crimson')}
			{@render ComponentSamples()}
		</div>
	{/snippet}
</Story>

<Story name="Expanded Palette — Semantic Roles">
	{#snippet template()}
		<div class="p-6 flex flex-col gap-6" style="background:var(--surface-bg)">
			<div>
				<p
					style="color:var(--text-strong);font-size:14px;font-weight:600;margin-bottom:4px;font-family:'JetBrains Mono Variable',monospace"
				>
					Semantic Role Expansion
				</p>
				<p style="color:var(--text-2);font-size:12px">
					Proposed additions to the token set — not theme-specific, usable across all themes.
					Legibility is the only constraint — no hue restrictions.
				</p>
			</div>

			<div class="p-4 rounded card-surface">
				<p
					style="color:var(--text-3);font-size:10px;letter-spacing:0.06em;margin-bottom:10px;text-transform:uppercase"
				>
					Current accent tokens (Forge)
				</p>
				<div class="flex gap-3 flex-wrap">
					{#each [{ label: 'green (success)', token: '--status-success-fg', desc: 'Success, active, fired' }, { label: 'amber (CTA/warn)', token: '--interactive-active-fg', desc: 'Primary CTA, warning, navigation' }, { label: 'red (danger)', token: '--status-danger-fg', desc: 'Error, destructive' }, { label: 'blue (info)', token: '--status-info-fg', desc: 'Info, focus ring, analysis' }, { label: 'rose (steering)', token: '--status-steering-fg', desc: 'Steerer zone identity' }, { label: 'teal (chart)', token: '--data-teal-fg', desc: 'Chart/data-viz only' }] as c (c.token)}
						<div class="flex items-center gap-2">
							<div class="h-5 w-5 rounded" style="background:var({c.token})"></div>
							<div>
								<p
									style="color:var(--text-strong);font-size:10px;font-family:'JetBrains Mono Variable',monospace"
								>
									{c.label}
								</p>
								<p style="color:var(--text-2);font-size:9px">{c.desc}</p>
							</div>
						</div>
					{/each}
				</div>
			</div>

			<div class="flex flex-col gap-4">
				<p
					style="color:var(--text-3);font-size:10px;letter-spacing:0.06em;text-transform:uppercase"
				>
					Proposed additions
				</p>

				<div class="p-4 rounded card-surface">
					<p style="color:var(--text-2);font-size:11px;margin-bottom:10px;font-weight:600">
						Green ramp — 3 steps (proposal: --green-bright, --green, --green-muted)
					</p>
					<p style="color:var(--text-3);font-size:10px;margin-bottom:10px">
						Use case: progress indicators, severity levels within success state. --green-bright for
						"just completed", --green for active/success, --green-muted for stale/acknowledged.
					</p>
					<div class="flex items-center gap-3">
						{#each [{ label: '--green-bright', bg: 'var(--green-bright)', desc: '3-step ramp: brightest' }, { label: '--status-success-fg', bg: 'var(--status-success-fg)', desc: '3-step ramp: mid' }, { label: '--green-muted', bg: 'var(--green-muted)', desc: '3-step ramp: dimmest — bg/border only' }] as step (step.label)}
							<div class="flex flex-col items-center gap-2">
								<div class="h-8 w-16 rounded" style="background:{step.bg}"></div>
								<p style="color:var(--text-3);font-size:9px;text-align:center;max-width:80px">
									{step.label}
								</p>
							</div>
						{/each}
					</div>
				</div>

				<div class="p-4 rounded card-surface">
					<p style="color:var(--text-2);font-size:11px;margin-bottom:10px;font-weight:600">
						--notice — advisory signal (not a warning, not info)
					</p>
					<p style="color:var(--text-3);font-size:10px;margin-bottom:10px">
						Gap: the current palette has no hue for "advisory" — a moderate priority between info
						(blue) and warning (amber). Proposed: warm purple-magenta that reads as "pay attention,
						not urgent." Use for onboarding callouts, feature announcements, soft nudges. Each theme
						defines its own --notice value. Legibility is the only constraint.
					</p>
					<div class="flex items-center gap-3">
						<div
							class="h-6 w-6 rounded flex-shrink-0"
							style="background:oklch(0.62 0.14 290)"
						></div>
						<div>
							<p
								style="color:var(--text-strong);font-size:11px;font-family:'JetBrains Mono Variable',monospace"
							>
								--notice (proposed) · oklch(0.62 0.14 290) · Forge example
							</p>
							<p style="color:var(--text-2);font-size:10px">
								Advisory role. Each theme provides its own value.
							</p>
						</div>
					</div>
				</div>

				<div class="p-4 rounded card-surface">
					<p style="color:var(--text-2);font-size:11px;margin-bottom:10px;font-weight:600">
						--surface-accent — tinted emphasis surface (beyond --surface and --surface-2)
					</p>
					<p style="color:var(--text-3);font-size:10px;margin-bottom:10px">
						Gap: no semantic surface for "highlighted section without using a full card border."
						Proposed: an accent-tinted surface variant, very subtle — sits between --surface and
						--surface-2 in brightness but with a barely perceptible hue tint from --primary. Use
						for: selected rows that aren't using the amber curly-brace treatment, onboarding panels,
						callout blocks.
					</p>
					<div class="flex items-center gap-3">
						<div
							class="h-10 w-24 rounded"
							style="background:var(--surface-2);border:1px solid var(--border-default)"
						></div>
						<div>
							<p
								style="color:var(--text-strong);font-size:11px;font-family:'JetBrains Mono Variable',monospace"
							>
								--surface-accent (proposed) — via --surface-2 here as placeholder
							</p>
							<p style="color:var(--text-2);font-size:10px">
								Subtle tint between --surface-card and --surface-2. Each theme provides its own.
							</p>
						</div>
					</div>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Theme Switcher Demo">
	{#snippet template()}
		<div class="flex flex-col gap-4" style="background:var(--surface-sidebar);padding:20px">
			<p style="color:var(--text-3);font-size:10px;letter-spacing:0.08em;text-transform:uppercase">
				Each panel below renders the SAME chrome using a different [data-theme] block from app.css.
				All text uses OKLCH tokens — no hardcoded hex values.
			</p>
			{#each THEMES as theme (theme.id)}
				<div
					data-theme={theme.id}
					class="rounded overflow-hidden"
					style="border:1px solid var(--border-default)"
				>
					<div
						class="flex items-center gap-3 px-4 h-11"
						style="background:var(--surface-sidebar);border-bottom:1px solid var(--border-subtle)"
					>
						<div
							class="h-2 w-2 rounded-full"
							style="background:var({theme.accentToken});box-shadow:0 0 8px var({theme.accentToken})"
						></div>
						<span
							style="font-family:'JetBrains Mono Variable',monospace;font-size:10px;color:var(--text-3);letter-spacing:0.12em"
							>CLAUDE PROXY INSPECTOR</span
						>
						<div class="ml-auto flex items-center gap-2">
							<div
								style="padding:2px 10px;border-radius:4px;font-size:10px;font-family:'JetBrains Mono Variable',monospace;background:var(--interactive-active-fg);color:var(--surface-sidebar);font-weight:600"
							>
								{theme.label}
							</div>
						</div>
					</div>

					<div class="flex h-24" style="background:var(--surface-bg)">
						<div
							class="w-48 h-full border-r px-3 py-3 flex flex-col gap-2"
							style="background:var(--surface-card);border-color:var(--border-default)"
						>
							{#each ['session-alfa', 'session-beta'] as sess, i (sess)}
								<div
									class="px-2 py-1 rounded"
									style={i === 0
										? `background:var(--interactive-active-bg);border:1px solid var(--interactive-active-border)`
										: `background:transparent`}
								>
									<p
										style="color:{i === 0
											? 'var(--text-strong)'
											: 'var(--text-2)'};font-size:10px;font-family:'JetBrains Mono Variable',monospace"
									>
										{sess}
									</p>
									<p style="color:var(--text-3);font-size:9px">3 requests</p>
								</div>
							{/each}
						</div>

						<div class="flex-1 px-4 py-3 flex flex-col gap-2" style="background:var(--surface-bg)">
							<p
								style="color:var(--text-strong);font-size:13px;font-family:'JetBrains Mono Variable',monospace;font-weight:600"
							>
								POST /v1/messages
							</p>
							<div class="flex items-center gap-3">
								<span
									style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-success-bg);color:var(--status-success-fg);border:1px solid var(--status-success-border)"
									><span
										style="width:5px;height:5px;border-radius:50%;background:var(--status-success-fg)"
									></span>Complete</span
								>
								<span style="color:var(--text-3);font-size:10px">1.24s · 4,812 tokens</span>
							</div>
							<p style="color:var(--text-2);font-size:11px">{theme.desc}</p>
						</div>
					</div>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

{#snippet ThemeHeader(label: string, desc: string, accent: string)}
	<div class="flex items-center gap-4">
		<div
			class="h-3 w-3 rounded-full"
			style="background:var({accent});box-shadow:0 0 10px var({accent})"
		></div>
		<div>
			<p
				style="color:var(--text-strong);font-size:16px;font-weight:700;letter-spacing:0.08em;font-family:'JetBrains Mono Variable',monospace"
			>
				{label}
			</p>
			<p style="color:var(--text-2);font-size:12px">{desc}</p>
		</div>
	</div>
{/snippet}

{#snippet TokenGrid(themeId?: string)}
	<div class="grid gap-2" style="grid-template-columns:repeat(auto-fill,minmax(220px,1fr))">
		{#each [{ group: 'Surfaces', tokens: SURFACE_TOKENS }, { group: 'Borders', tokens: BORDER_TOKENS }, { group: 'Text', tokens: TEXT_TOKENS }, { group: 'Accents', tokens: ACCENT_TOKENS }, { group: 'Color Ramps (bright/muted)', tokens: RAMP_TOKENS }, { group: 'Extended Roles (notice/pine/gold)', tokens: EXTENDED_TOKENS }] as group (group.group)}
			<div
				class="p-3 rounded"
				style="background:var(--surface-card);border:1px solid var(--border-default)"
			>
				<p
					style="color:var(--text-3);font-size:9px;letter-spacing:0.06em;text-transform:uppercase;margin-bottom:8px"
				>
					{group.group}
				</p>
				<div class="flex flex-col gap-2">
					{#each group.tokens as tok (tok)}
						<div class="flex items-center gap-2">
							<div class="h-4 w-4 rounded flex-shrink-0" style="background:var({tok})"></div>
							<span
								style="color:var(--text-1);font-size:10px;font-family:'JetBrains Mono Variable',monospace;flex:1"
								>{tok}</span
							>
							<span
								style="color:var(--text-3);font-size:9px;font-family:'JetBrains Mono Variable',monospace"
								>{(themeId ? (perThemeTokens[themeId] ?? resolvedTokens) : resolvedTokens)[tok] ??
									'…'}</span
							>
						</div>
					{/each}
				</div>
			</div>
		{/each}
	</div>
{/snippet}

{#snippet ComponentSamples()}
	<div
		class="p-4 rounded"
		style="background:var(--surface-card);border:1px solid var(--border-default)"
	>
		<p
			style="color:var(--text-3);font-size:10px;letter-spacing:0.06em;margin-bottom:12px;text-transform:uppercase"
		>
			Component samples in this theme
		</p>

		<div class="flex items-center gap-2 flex-wrap mb-4">
			<span
				style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-success-bg);color:var(--status-success-fg);border:1px solid var(--status-success-border)"
			>
				<span
					style="width:5px;height:5px;border-radius:50%;flex-shrink:0;background:var(--status-success-fg)"
				></span>Active
			</span>
			<span
				style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-warning-bg);color:var(--status-warning-fg);border:1px solid var(--status-warning-border)"
				>Warning</span
			>
			<span
				style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-danger-bg);color:var(--status-danger-fg);border:1px solid var(--status-danger-border)"
				>Error</span
			>
			<span
				style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-info-bg);color:var(--status-info-fg);border:1px solid var(--status-info-border)"
				>Analyzed</span
			>
			<span
				style="display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:500;padding:2px 7px;border-radius:4px;background:var(--status-muted-bg);color:var(--text-3);border:1px solid var(--status-muted-border)"
				>Muted</span
			>
		</div>

		<div class="flex items-center gap-2 flex-wrap mb-4">
			<span
				style="display:inline-block;padding:2px 8px;border-radius:9999px;font-size:var(--text-label,11px);font-weight:500;background:var(--status-success-bg);color:var(--status-success-fg);box-shadow:inset 0 0 0 1px var(--status-success-border)"
				>complete</span
			>
			<span
				style="display:inline-block;padding:2px 8px;border-radius:9999px;font-size:var(--text-label,11px);font-weight:500;background:var(--status-warning-bg);color:var(--status-warning-fg);box-shadow:inset 0 0 0 1px var(--status-warning-border)"
				>streaming</span
			>
			<span
				style="display:inline-block;padding:2px 8px;border-radius:9999px;font-size:var(--text-label,11px);font-weight:500;background:var(--status-danger-bg);color:var(--status-danger-fg);box-shadow:inset 0 0 0 1px var(--status-danger-border)"
				>error</span
			>
			<span
				style="display:inline-block;padding:2px 8px;border-radius:9999px;font-size:var(--text-label,11px);font-weight:500;background:var(--status-muted-bg);color:var(--text-3);box-shadow:inset 0 0 0 1px var(--status-muted-border)"
				>unknown</span
			>
		</div>

		<div class="flex items-center gap-3 flex-wrap mb-4">
			<button
				style="padding:5px 14px;border-radius:6px;font-size:11px;font-family:'JetBrains Mono Variable',monospace;background:var(--interactive-active-fg);color:var(--surface-sidebar);font-weight:600;border:none;cursor:default"
			>
				Primary CTA
			</button>
			<button
				style="padding:5px 14px;border-radius:6px;font-size:11px;font-family:'JetBrains Mono Variable',monospace;background:transparent;color:var(--text-1);border:1px solid var(--border-default);cursor:default"
			>
				Outline
			</button>
			<button
				style="padding:5px 14px;border-radius:6px;font-size:11px;font-family:'JetBrains Mono Variable',monospace;background:var(--surface-2);color:var(--text-2);border:none;cursor:default"
			>
				Secondary
			</button>
		</div>

		<div
			class="p-3 rounded"
			style="background:var(--surface-2);border:1px solid var(--border-subtle)"
		>
			<div class="flex items-center justify-between mb-2">
				<span
					style="color:var(--text-strong);font-size:12px;font-family:'JetBrains Mono Variable',monospace;font-weight:600"
					>POST /v1/messages</span
				>
				<span
					style="display:inline-block;padding:2px 8px;border-radius:9999px;font-size:var(--text-label,11px);font-weight:500;background:var(--status-success-bg);color:var(--status-success-fg);box-shadow:inset 0 0 0 1px var(--status-success-border)"
					>complete</span
				>
			</div>
			<div class="flex items-center gap-3">
				<span style="color:var(--text-3);font-size:10px">2026-06-16 14:23:01</span>
				<span style="color:var(--text-faint)">·</span>
				<span style="color:var(--text-2);font-size:10px">1.24s</span>
				<span style="color:var(--text-faint)">·</span>
				<span style="color:var(--text-2);font-size:10px">4,812 tokens</span>
			</div>
		</div>

		<div class="flex flex-col gap-1 mt-4 p-2 rounded" style="background:var(--surface-sidebar)">
			<div class="rail-link active">Sessions<span class="rail-link-count">24</span></div>
			<div class="rail-link">Analysis</div>
			<div class="rail-link subtle">Settings</div>
		</div>
	</div>
{/snippet}
