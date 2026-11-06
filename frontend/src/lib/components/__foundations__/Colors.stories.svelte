<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Foundations/Colors',
		tags: ['autodocs']
	});

	type Swatch = { token: string; usage: string };

	const SURFACES: Swatch[] = [
		{ token: '--bg', usage: 'Page ground. Nothing sits behind it.' },
		{ token: '--surface-1', usage: 'Cards, panels — the default raised plane.' },
		{ token: '--surface-2', usage: 'Nested panels, table header rows, inset wells.' },
		{ token: '--surface-3', usage: 'Highest step — popovers, hover fills on surface-2.' }
	];

	const LINES: Swatch[] = [
		{ token: '--line', usage: 'Default hairline between regions.' },
		{ token: '--line-strong', usage: 'Emphasis divider, active edge, focused container.' }
	];

	const TEXT: Swatch[] = [
		{ token: '--text-1', usage: 'Body and default content.' },
		{ token: '--text-2', usage: 'Secondary labels, metadata.' },
		{ token: '--text-3', usage: 'Tertiary, muted, uppercase section labels.' }
	];

	const HUES = ['cyan', 'magenta', 'green', 'red', 'blue', 'yellow'] as const;

	const SIGNALS: Swatch[] = [
		{ token: '--signal-active', usage: 'Active / nav — the thing you are on.' },
		{ token: '--signal-live', usage: 'Live, streaming, in-flight.' },
		{ token: '--signal-complete', usage: 'Completed, succeeded, settled.' },
		{ token: '--signal-error', usage: 'Failed, errored, destructive.' },
		{ token: '--signal-info', usage: 'Informational, neutral notice.' },
		{ token: '--signal-warn', usage: 'Warning, degraded, needs attention.' },
		{ token: '--accent-primary', usage: 'The system accent.' },
		{ token: '--focus', usage: 'Focus ring.' }
	];

	const FAMILIES = [
		{ name: 'success', usage: 'Completed work, healthy state.' },
		{ name: 'warning', usage: 'Degraded, approaching a limit.' },
		{ name: 'danger', usage: 'Errors and destructive actions — red AT REST, not on hover.' },
		{ name: 'info', usage: 'Neutral notices.' },
		{ name: 'steering', usage: 'Steering and intervention surfaces.' },
		{ name: 'muted', usage: 'Recessive rows — a dimmer surface, never lowered opacity.' }
	];

	const ALIASES = [
		{ token: '--role-tool-fg', usage: 'Tool-loop rows.' },
		{
			token: '--data-teal-fg',
			usage: 'Legacy NAME only — resolves to the cyan above. There is no teal.'
		}
	];

	const IDENT_PARTS = ['fg', 'fill', 'line', 'plate'] as const;
	const IDENTITY: Swatch[] = [
		{ token: 'session', usage: "The operator's own session — and the terminal, same process." },
		{ token: 'agent', usage: 'A delegated agent, and the agents group.' },
		{ token: 'tool', usage: 'A tool family or an MCP server.' },
		{ token: 'mike', usage: "Mike's own chips on the strip." },
		{ token: 'outside', usage: 'Beyond the boundary, unbranded.' },
		{ token: 'terminal', usage: "The operator's terminal." }
	];
</script>

<script lang="ts">
	import { onMount } from 'svelte';

	let hex = $state<Record<string, string>>({});

	onMount(() => {
		const cs = getComputedStyle(document.documentElement);
		const tokens = [
			...SURFACES.map((s) => s.token),
			...LINES.map((s) => s.token),
			...TEXT.map((s) => s.token),
			...SIGNALS.map((s) => s.token),
			...ALIASES.map((s) => s.token),
			...HUES.flatMap((h) => [
				`--funk-${h}`,
				`--funk-${h}-dim`,
				`--funk-${h}-border`,
				`--funk-chip-${h}`
			]),
			...FAMILIES.flatMap((f) => [
				`--status-${f.name}-bg`,
				`--status-${f.name}-border`,
				`--status-${f.name}-fg`
			])
		];
		hex = Object.fromEntries(tokens.map((t) => [t, cs.getPropertyValue(t).trim()]));

		const probe = document.createElement('div');
		probe.setAttribute('data-design', 'funk');
		document.body.appendChild(probe);
		const funkCs = getComputedStyle(probe);
		const funkTokens = [
			...IDENTITY.flatMap((i) => IDENT_PARTS.map((p) => `--ident-${i.token}-${p}`)),
			'--vendor-anthropic'
		];
		hex = {
			...hex,
			...Object.fromEntries(funkTokens.map((t) => [t, funkCs.getPropertyValue(t).trim()]))
		};
		probe.remove();
	});
</script>

{#snippet Note(text: string)}
	<p style="color:var(--text-3);font-size:11px;line-height:1.5;margin:0 0 14px;max-width:62ch">
		{text}
	</p>
{/snippet}

{#snippet Heading(text: string)}
	<h3
		style="color:var(--text-1);font-size:13px;letter-spacing:.06em;text-transform:uppercase;margin:0 0 6px"
	>
		{text}
	</h3>
{/snippet}

{#snippet Row(token: string, usage: string, swatchStyle: string)}
	<div
		style="display:flex;align-items:center;gap:14px;padding:8px 0;border-bottom:1px solid var(--line)"
	>
		<div style="{swatchStyle};width:56px;height:36px;border-radius:2px;flex:none"></div>
		<div style="min-width:0">
			<code style="color:var(--text-1);font-size:12px">{token}</code>
			<span style="color:var(--text-3);font-size:11px;margin-left:8px"
				>{hex[token] || 'unresolved'}</span
			>
			<p style="color:var(--text-2);font-size:11px;margin:2px 0 0">{usage}</p>
		</div>
	</div>
{/snippet}

<Story name="Surfaces & Lines">
	<div style="background:var(--bg);padding:24px">
		{@render Heading('Surfaces')}
		{@render Note(
			'Four steps, low contrast between them on purpose — depth reads from the hairline, not from a big luminance jump.'
		)}
		{#each SURFACES as s (s.token)}
			{@render Row(s.token, s.usage, `background:var(${s.token});border:1px solid var(--line)`)}
		{/each}

		<div style="height:28px"></div>
		{@render Heading('Lines')}
		{#each LINES as s (s.token)}
			{@render Row(s.token, s.usage, `background:var(${s.token})`)}
		{/each}
	</div>
</Story>

<Story name="Text">
	<div style="background:var(--bg);padding:24px">
		{@render Heading('Text ramp')}
		{@render Note(
			'Three steps. Judge legibility by looking at the running app, never by a measured ratio.'
		)}
		{#each TEXT as t (t.token)}
			<div style="padding:10px 0;border-bottom:1px solid var(--line)">
				<p style="color:var({t.token});font-size:14px;margin:0 0 3px">
					The quick brown fox jumps over the lazy dog — {t.token}
				</p>
				<code style="color:var(--text-3);font-size:11px"
					>{t.token} · {hex[t.token] || 'unresolved'}</code
				>
				<span style="color:var(--text-3);font-size:11px;margin-left:8px">{t.usage}</span>
			</div>
		{/each}
	</div>
</Story>

<Story name="Accents">
	<div style="background:var(--bg);padding:24px">
		{@render Heading('The six FUNK hues')}
		{@render Note(
			'Six accents, and none of them is teal. Each ships five forms at the same step: base, dim (fill), border, chip (an OPAQUE tint pre-composited over surface-1 — never a translucent wash, so nothing paints through text), and glow. Components should reference the semantic role in the next story, not these hue names.'
		)}
		<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:16px">
			{#each HUES as h (h)}
				<div style="border:1px solid var(--line);padding:12px;background:var(--surface-1)">
					<div style="display:flex;gap:6px;margin-bottom:10px">
						<div style="background:var(--funk-{h});height:44px;flex:2;border-radius:2px"></div>
						<div style="background:var(--funk-chip-{h});height:44px;flex:1;border-radius:2px"></div>
						<div
							style="background:var(--funk-{h}-dim);border:1px solid var(--funk-{h}-border);height:44px;flex:1;border-radius:2px"
						></div>
					</div>
					<code style="color:var(--funk-{h});font-size:12px">--{h}</code>
					<p style="color:var(--text-3);font-size:11px;margin:3px 0 0">
						{hex[`--funk-${h}`] || 'unresolved'}
					</p>
					<p style="color:var(--text-3);font-size:10px;margin:6px 0 0;line-height:1.5">
						chip {hex[`--funk-chip-${h}`] || 'unresolved'}<br />
						border {hex[`--funk-${h}-border`] || 'unresolved'}
					</p>
				</div>
			{/each}
		</div>
	</div>
</Story>

<Story name="Semantic Roles">
	<div style="background:var(--bg);padding:24px">
		{@render Heading('Signals — what components reference')}
		{@render Note(
			'Reach for the role, not the hue. A row is not "the cyan one", it is the active one; recolouring the system then stays a one-line change.'
		)}
		{#each SIGNALS as s (s.token)}
			{@render Row(s.token, s.usage, `background:var(${s.token})`)}
		{/each}

		<div style="height:28px"></div>
		{@render Heading('Aliases')}
		{#each ALIASES as a (a.token)}
			{@render Row(a.token, a.usage, `background:var(${a.token})`)}
		{/each}
	</div>
</Story>

<Story name="Status Families">
	<div style="background:var(--bg);padding:24px">
		{@render Heading('Triplets — bg / border / fg move together')}
		{@render Note(
			'Each family is a set. Take all three from the same family; a border from one and a foreground from another is how a surface stops reading as a single state.'
		)}
		<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:14px">
			{#each FAMILIES as f (f.name)}
				<div
					style="background:var(--status-{f.name}-bg);border:1px solid var(--status-{f.name}-border);padding:12px;border-radius:2px"
				>
					<code style="color:var(--status-{f.name}-fg);font-size:12px">--status-{f.name}-*</code>
					<p style="color:var(--text-2);font-size:11px;margin:6px 0 0;line-height:1.5">{f.usage}</p>
					<p style="color:var(--text-3);font-size:10px;margin:6px 0 0;line-height:1.5">
						fg {hex[`--status-${f.name}-fg`] || 'unresolved'}<br />
						bg {hex[`--status-${f.name}-bg`] || 'unresolved'}<br />
						border {hex[`--status-${f.name}-border`] || 'unresolved'}
					</p>
				</div>
			{/each}
		</div>
	</div>
</Story>

<Story name="Identity — who">
	<div data-design="funk" style="background:var(--bg);padding:24px">
		{@render Heading('Identity — who')}
		{@render Note(
			"Identity says WHO a chip belongs to — plate, fill, ring at rest, and the sub line all move together, and every trace wears its sender's fg. Signals (two stories up) say STATE, never WHO. No signal blue here: it is not tellable from cyan at chip scale."
		)}
		<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:16px">
			{#each IDENTITY as fam (fam.token)}
				<div style="border:1px solid var(--line);padding:12px;background:var(--surface-1)">
					<div style="display:flex;gap:6px;margin-bottom:10px">
						{#each IDENT_PARTS as part (part)}
							<div
								style="background:var(--ident-{fam.token}-{part});height:44px;flex:1;border-radius:2px"
							></div>
						{/each}
					</div>
					<code style="color:var(--ident-{fam.token}-fg);font-size:12px">--ident-{fam.token}-*</code
					>
					<p style="color:var(--text-2);font-size:11px;margin:6px 0 0;line-height:1.5">
						{fam.usage}
					</p>
					<p style="color:var(--text-3);font-size:10px;margin:6px 0 0;line-height:1.5">
						{#each IDENT_PARTS as part (part)}
							{part} {hex[`--ident-${fam.token}-${part}`] || 'unresolved'}<br />
						{/each}
					</p>
				</div>
			{/each}
		</div>

		<div style="height:28px"></div>
		{@render Heading('Vendor')}
		{@render Note(
			"The vendor's own colour for a response coming back — not a signal, not an identity family. Anthropic only."
		)}
		<div style="max-width:220px">
			<div style="border:1px solid var(--line);padding:12px;background:var(--surface-1)">
				<div
					style="background:var(--vendor-anthropic);height:44px;border-radius:2px;margin-bottom:10px"
				></div>
				<code style="color:var(--vendor-anthropic);font-size:12px">--vendor-anthropic</code>
				<p style="color:var(--text-3);font-size:11px;margin:3px 0 0">
					{hex['--vendor-anthropic'] || 'unresolved'}
				</p>
			</div>
		</div>
	</div>
</Story>
