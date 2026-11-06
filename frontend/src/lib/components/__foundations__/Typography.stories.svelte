<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Foundations/Typography',
		tags: ['autodocs']
	});

	const LOREM = 'The quick brown fox jumps over the lazy dog';
	const DATA_SAMPLE = '1,247ms · 200 OK · 4,096 tokens';
</script>

<Story name="FontFamilies">
	{#snippet template()}
		<div class="flex flex-col gap-8 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Font Families — All 5</p>

			<div class="card-surface p-6 flex flex-col gap-3">
				<div class="flex items-center gap-3">
					<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
						JetBrains Mono Variable — Data &amp; Code
					</p>
					<span class="badge-nx badge-active">PRIMARY</span>
				</div>
				<p class="text-base text-[var(--text-1)]">{LOREM}</p>
				<p class="text-sm text-[var(--text-2)]">{DATA_SAMPLE}</p>
				<p class="text-caption text-[var(--text-3)]">
					Applied globally on <code>body</code>. Use for all numbers, timestamps, HTTP status, JSON,
					metrics, latency.
				</p>
				<div class="flex flex-col gap-1">
					<code class="text-caption text-[var(--interactive-active-fg)]"
						>font-family: 'JetBrains Mono Variable', 'JetBrains Mono', ui-monospace, monospace</code
					>
					<code class="text-caption text-[var(--text-3)]"
						>CSS: default body — no class needed · Tailwind: font-mono</code
					>
					<code class="text-caption text-[var(--text-faint)]"
						>source: src/routes/+layout.svelte · @fontsource-variable/jetbrains-mono</code
					>
				</div>
			</div>

			<div class="card-surface p-6 flex flex-col gap-3">
				<div class="flex items-center gap-3">
					<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
						Syne Variable — Display &amp; Headings
					</p>
					<span class="badge-nx badge-analyzed">DISPLAY</span>
				</div>
				<h2
					class="text-[var(--text-strong)]"
					style="font-family: var(--font-display); font-size: 22px; font-weight: 700; letter-spacing: -0.02em;"
				>
					Active Sessions
				</h2>
				<h3
					class="text-[var(--text-strong)]"
					style="font-family: var(--font-display); font-size: 15px; font-weight: 600; letter-spacing: -0.01em;"
				>
					Request Details
				</h3>
				<p
					class="text-[var(--text-1)]"
					style="font-family: var(--font-display); font-size: 30px; font-weight: 700; letter-spacing: -0.02em; line-height: 1;"
				>
					4,218
				</p>
				<p class="text-caption text-[var(--text-3)]">
					Use for section titles, KPI category labels, column headers, large numeric KPI values.
					Never for body text or data values.
				</p>
				<div class="flex flex-col gap-1">
					<code class="text-caption text-[var(--interactive-active-fg)]"
						>font-family: var(--font-display) → 'Syne Variable', 'Syne', sans-serif</code
					>
					<code class="text-caption text-[var(--text-3)]"
						>CSS: var(--font-display) · Tailwind: font-display</code
					>
					<code class="text-caption text-[var(--text-faint)]"
						>source: src/routes/+layout.svelte · @fontsource-variable/syne</code
					>
				</div>
			</div>

			<div class="card-surface p-6 flex flex-col gap-3">
				<div class="flex items-center gap-3">
					<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
						Inter Variable — UI Chrome &amp; Sans-Serif
					</p>
					<span class="badge-nx badge-muted">SANS</span>
				</div>
				<p class="text-base text-[var(--text-1)]" style="font-family: var(--font-sans);">{LOREM}</p>
				<p class="text-sm text-[var(--text-2)]" style="font-family: var(--font-sans);">
					Settings panel · Prose descriptions · Help text
				</p>
				<p
					class="text-xs text-[var(--text-3)]"
					style="font-family: var(--font-sans); letter-spacing: 0.04em; text-transform: uppercase;"
				>
					FILTER RULES · SESSIONS · ANALYSIS
				</p>
				<p class="text-caption text-[var(--text-3)]">
					Use for prose-heavy UI surfaces (help drawers, settings, onboarding text) where the
					monospace body font feels too technical. Excellent optical balance at 12–14px.
				</p>
				<div class="flex flex-col gap-1">
					<code class="text-caption text-[var(--interactive-active-fg)]"
						>font-family: var(--font-sans) → 'Inter Variable', 'Inter', system-ui, sans-serif</code
					>
					<code class="text-caption text-[var(--text-3)]"
						>CSS: var(--font-sans) · Tailwind: font-sans</code
					>
					<code class="text-caption text-[var(--text-faint)]"
						>source: src/routes/+layout.svelte · @fontsource-variable/inter</code
					>
				</div>
			</div>

			<div class="card-surface p-6 flex flex-col gap-3">
				<div class="flex items-center gap-3">
					<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
						Geist Mono Variable — Alternative Mono
					</p>
					<span class="badge-nx badge-muted">ALT MONO</span>
				</div>
				<p class="text-base text-[var(--text-1)]" style="font-family: var(--font-mono-alt);">
					{LOREM}
				</p>
				<p class="text-sm text-[var(--text-2)]" style="font-family: var(--font-mono-alt);">
					{DATA_SAMPLE}
				</p>
				<p class="text-caption text-[var(--text-3)]" style="font-family: var(--font-mono-alt);">
					Cleaner at micro sizes (9–11px) — less visual noise than JetBrains in dense panels.
				</p>
				<p class="text-caption text-[var(--text-3)]">
					Use in code panels, diff views, or dense data grids where JetBrains Mono's personality is
					too strong. More neutral than JetBrains but still unmistakably monospaced.
				</p>
				<div class="flex flex-col gap-1">
					<code class="text-caption text-[var(--interactive-active-fg)]"
						>font-family: var(--font-mono-alt) → 'Geist Mono Variable', 'Geist Mono', ui-monospace,
						monospace</code
					>
					<code class="text-caption text-[var(--text-3)]"
						>CSS: var(--font-mono-alt) · Tailwind: font-mono-alt</code
					>
					<code class="text-caption text-[var(--text-faint)]"
						>source: src/routes/+layout.svelte · @fontsource-variable/geist-mono</code
					>
				</div>
			</div>

			<div class="card-surface p-6 flex flex-col gap-3">
				<div class="flex items-center gap-3">
					<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
						Iosevka Term — Condensed Mono (system fallback)
					</p>
					<span
						class="badge-nx"
						style="background: var(--surface-2); color: var(--text-faint); border: 1px solid var(--border-subtle);"
						>FALLBACK</span
					>
				</div>
				<p class="text-base text-[var(--text-1)]" style="font-family: var(--font-mono-condensed);">
					{LOREM}
				</p>
				<p
					class="text-caption text-[var(--text-3)]"
					style="font-family: var(--font-mono-condensed);"
				>
					Falls back to JetBrains Mono Variable when Iosevka Term is not installed.
				</p>
				<p class="text-caption text-[var(--text-3)]">
					No npm package available — system-only. Renders Iosevka's very tight horizontal rhythm if
					installed; otherwise degrades gracefully to JetBrains Mono. Useful for extremely
					column-dense views.
				</p>
				<div class="flex flex-col gap-1">
					<code class="text-caption text-[var(--interactive-active-fg)]"
						>font-family: var(--font-mono-condensed) → 'Iosevka Term', 'JetBrains Mono Variable',
						ui-monospace</code
					>
					<code class="text-caption text-[var(--text-3)]"
						>CSS: var(--font-mono-condensed) · no Tailwind utility (rarely used)</code
					>
					<code class="text-caption text-[var(--text-faint)]"
						>source: app.css --font-mono-condensed · system install only</code
					>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="FontPairing">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Font Pairings in Context</p>

			<div class="card-surface p-5 flex flex-col gap-3">
				<p class="text-caption text-[var(--text-3)]">
					Column header (Syne) + data row (JetBrains Mono)
				</p>
				<div class="flex flex-col gap-1">
					<div class="flex items-center gap-8 border-b border-[var(--border-subtle)] pb-2">
						<span
							style="font-family: var(--font-display); font-size: 11px; font-weight: 600; letter-spacing: -0.01em; color: var(--text-strong);"
							>Session</span
						>
						<span
							style="font-family: var(--font-display); font-size: 11px; font-weight: 600; letter-spacing: -0.01em; color: var(--text-strong);"
							>Requests</span
						>
						<span
							style="font-family: var(--font-display); font-size: 11px; font-weight: 600; letter-spacing: -0.01em; color: var(--text-strong);"
							>Duration</span
						>
						<span
							style="font-family: var(--font-display); font-size: 11px; font-weight: 600; letter-spacing: -0.01em; color: var(--text-strong);"
							>Tokens</span
						>
					</div>
					<div class="flex items-center gap-8 py-1">
						<span class="text-xs text-[var(--text-1)]">abc12345</span>
						<span class="text-xs text-[var(--text-1)]" style="padding-left: 4px;">47</span>
						<span class="text-xs text-[var(--text-2)]" style="padding-left: 8px;">3m 12s</span>
						<span class="text-xs text-[var(--text-1)]" style="padding-left: 4px;">128,447</span>
					</div>
				</div>
			</div>

			<div class="card-surface p-5 flex flex-col gap-3">
				<p class="text-caption text-[var(--text-3)]">
					KPI card — Syne large value + JetBrains Mono label
				</p>
				<div class="flex gap-8">
					<div class="flex flex-col gap-1">
						<span class="text-label uppercase tracking-kpi text-[var(--text-3)]">Requests</span>
						<span
							style="font-family: var(--font-display); font-size: 28px; font-weight: 700; letter-spacing: -0.02em; line-height: 1; color: var(--text-strong);"
							>4,218</span
						>
					</div>
					<div class="flex flex-col gap-1">
						<span class="text-label uppercase tracking-kpi text-[var(--text-3)]">Avg Latency</span>
						<span
							style="font-family: var(--font-display); font-size: 28px; font-weight: 700; letter-spacing: -0.02em; line-height: 1; color: var(--interactive-active-fg);"
							>847ms</span
						>
					</div>
					<div class="flex flex-col gap-1">
						<span class="text-label uppercase tracking-kpi text-[var(--text-3)]">Sessions</span>
						<span
							style="font-family: var(--font-display); font-size: 28px; font-weight: 700; letter-spacing: -0.02em; line-height: 1; color: var(--data-teal-fg);"
							>12</span
						>
					</div>
				</div>
			</div>

			<div class="card-surface p-5 flex flex-col gap-3">
				<p class="text-caption text-[var(--text-3)]">
					Help text (Inter) + command example (JetBrains Mono)
				</p>
				<p
					class="text-sm text-[var(--text-1)] leading-relaxed"
					style="font-family: var(--font-sans);"
				>
					The proxy captures every request your Claude session makes. Each request body, response
					stream, and tool call is persisted for later inspection. Use the filter rules to redact
					sensitive system-prompt blocks before they reach the model.
				</p>
				<code
					class="text-xs text-[var(--interactive-active-fg)] bg-[var(--surface-2)] px-3 py-2 rounded"
				>
					ANTHROPIC_BASE_URL=http://localhost:9988 claude
				</code>
			</div>

			<div class="card-surface p-5 flex flex-col gap-3">
				<p class="text-caption text-[var(--text-3)]">
					Section title (Syne) + code panel (Geist Mono)
				</p>
				<h3
					class="text-[var(--text-strong)]"
					style="font-family: var(--font-display); font-size: 13px; font-weight: 600; letter-spacing: -0.01em;"
				>
					Request Body
				</h3>
				<div
					class="bg-[var(--surface-2)] border border-[var(--border-default)] rounded-md p-3"
					style="font-family: var(--font-mono-alt); font-size: 11px; line-height: 1.65; color: var(--text-1);"
				>
					{`{
  "model": "claude-opus-4-8-20251101",
  "max_tokens": 8096,
  "system": [...],
  "messages": [...]
}`}
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="TypeScale">
	{#snippet template()}
		<div class="flex flex-col gap-3 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Type Scale</p>

			<div class="card-surface p-4 flex flex-col gap-3">
				<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
					Micro Scale (phase 2.9)
				</p>

				<div class="flex items-baseline gap-4">
					<span class="text-micro text-[var(--text-1)]">{LOREM}</span>
					<code class="text-caption text-[var(--text-3)]"
						>text-micro · 9px · hash chips, rail annotations</code
					>
				</div>
				<div class="flex items-baseline gap-4">
					<span class="text-caption text-[var(--text-1)]">{LOREM}</span>
					<code class="text-caption text-[var(--text-3)]"
						>text-caption · 10px · sidebar meta, fold controls</code
					>
				</div>
				<div class="flex items-baseline gap-4">
					<span class="text-label text-[var(--text-1)]">{LOREM}</span>
					<code class="text-caption text-[var(--text-3)]"
						>text-label · 11px · badges, table headers</code
					>
				</div>
			</div>

			<div class="card-surface p-4 flex flex-col gap-3">
				<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
					Tailwind Built-In Scale
				</p>

				<div class="flex items-baseline gap-4">
					<span class="text-xs text-[var(--text-1)]">{LOREM}</span>
					<code class="text-caption text-[var(--text-3)]">text-xs · 12px</code>
				</div>
				<div class="flex items-baseline gap-4">
					<span class="text-sm text-[var(--text-1)]">{LOREM}</span>
					<code class="text-caption text-[var(--text-3)]">text-sm · 13px</code>
				</div>
				<div class="flex items-baseline gap-4">
					<span class="text-base text-[var(--text-1)]">{LOREM}</span>
					<code class="text-caption text-[var(--text-3)]">text-base · body default</code>
				</div>
				<div class="flex items-baseline gap-4">
					<span class="text-lg text-[var(--text-1)]">{LOREM}</span>
					<code class="text-caption text-[var(--text-3)]">text-lg</code>
				</div>
			</div>

			<div class="card-surface p-4 flex flex-col gap-3">
				<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
					KPI Value (Syne Variable)
				</p>
				<div class="flex items-baseline gap-4">
					<span
						class="text-[var(--text-1)]"
						style="font-family: var(--font-display); font-size: 30px; font-weight: 700; letter-spacing: -0.02em; line-height: 1;"
					>
						4,218
					</span>
					<code class="text-caption text-[var(--text-3)]"
						>Syne Variable 30px 700 -0.02em · KPI numeric values</code
					>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Tracking">
	{#snippet template()}
		<div class="flex flex-col gap-4 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Letter Spacing Tokens</p>

			<div class="card-surface p-4 flex flex-col gap-4">
				<div class="flex items-center gap-4">
					<span class="text-label uppercase tracking-kpi text-[var(--text-3)] min-w-32">
						TABLE HEADER
					</span>
					<code class="text-caption text-[var(--text-3)]">
						tracking-kpi · 0.04em · table headers and KPI labels above numeric values
					</code>
				</div>
				<div class="flex items-center gap-4">
					<span class="text-label uppercase tracking-wider text-[var(--text-3)] min-w-32">
						CHIP LABEL
					</span>
					<code class="text-caption text-[var(--text-3)]">
						tracking-wider · 0.05em · standalone chips and sidebar group labels
					</code>
				</div>
				<div class="flex items-center gap-4">
					<span
						class="tracking-tight text-[var(--text-strong)] min-w-32"
						style="font-family: var(--font-display); font-size: 22px; font-weight: 700;"
					>
						Section Title
					</span>
					<code class="text-caption text-[var(--text-3)]">
						tracking-tight · display headings in Syne Variable
					</code>
				</div>
				<div class="flex items-center gap-4">
					<span
						class="text-label uppercase tracking-kpi text-[var(--text-3)] min-w-32"
						style="font-family: var(--font-sans);"
					>
						SECTION LABEL
					</span>
					<code class="text-caption text-[var(--text-3)]">
						Inter + tracking-kpi · works equally well for sans-serif labels
					</code>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="WrongRight">
	{#snippet template()}
		<div class="flex flex-col gap-8 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				Common Typography Mistakes
			</p>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — arbitrary px bypasses type scale
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;span class="text-[10px]"&gt; · &lt;span class="text-[11px]"&gt; · &lt;span
						class="text-[12px]"&gt;
					</code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">RIGHT — named utilities</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;span class="text-caption"&gt; (10px) · &lt;span class="text-label"&gt; (11px) ·
						&lt;span class="text-xs"&gt; (12px)
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]"></p>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">WRONG — arbitrary tracking</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;th class="tracking-[0.04em]"&gt; · &lt;span class="tracking-[0.05em]"&gt;
					</code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">RIGHT — named tokens</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;th class="tracking-kpi"&gt; · &lt;span class="tracking-wider"&gt;
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]"></p>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — mono font for heading
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;h2 class="text-lg font-semibold"&gt;Active Sessions&lt;/h2&gt; &lt;!-- defaults to
						JetBrains Mono --&gt;
					</code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">RIGHT — Syne for headings</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;h2 class="font-display text-lg font-semibold"&gt;Active Sessions&lt;/h2&gt;
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]">
					Why: JetBrains Mono is for data; Syne is for identity and emphasis.
				</p>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — hardcoded font-family string
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						style="font-family: 'Syne', sans-serif"
					</code>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">
					RIGHT — CSS token or Tailwind utility
				</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						style="font-family: var(--font-display)" &nbsp;&nbsp;or&nbsp;&nbsp; class="font-display"
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]">
					Why: hardcoded strings break if the font is renamed or a variable fallback is needed. CSS
					tokens let you change the stack in one place. The token also includes the correct fallback
					chain (Variable → named → system).
				</p>
			</div>

			<div class="flex flex-col gap-2">
				<p class="text-xs font-medium text-[var(--status-danger-fg)]">
					WRONG — mono for prose descriptions
				</p>
				<div class="card-surface p-3">
					<p class="text-xs text-[var(--text-2)]">
						&lt;p class="text-sm"&gt;The proxy captures every request...&lt;/p&gt; &lt;!-- defaults
						to JetBrains Mono — reads as code, not prose --&gt;
					</p>
				</div>
				<p class="text-xs font-medium text-[var(--status-success-fg)]">RIGHT — Inter for prose</p>
				<div class="card-surface p-3">
					<code class="text-xs text-[var(--text-2)]">
						&lt;p class="font-sans text-sm"&gt;The proxy captures every request...&lt;/p&gt;
					</code>
				</div>
				<p class="text-caption text-[var(--text-3)]">
					Why: JetBrains Mono signals "data/code"; Inter signals "prose/UI". Use Inter for help
					text, onboarding copy, and any surface where reading ease matters more than data fidelity.
				</p>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="QuickReference">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8 bg-[var(--surface-bg)]">
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Font Quick Reference</p>

			<div class="card-surface p-5 flex flex-col gap-3">
				<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">Which font for what?</p>

				<div class="flex flex-col gap-2">
					<div
						class="grid gap-3 text-label uppercase tracking-kpi text-[var(--text-3)] border-b border-[var(--border-subtle)] pb-2"
						style="grid-template-columns: 1fr 1fr 1fr 2fr;"
					>
						<span>Font</span>
						<span>CSS var</span>
						<span>Tailwind</span>
						<span>Use for</span>
					</div>

					<div
						class="grid gap-3 text-xs text-[var(--text-2)] py-1 border-b border-[var(--border-subtle)]"
						style="grid-template-columns: 1fr 1fr 1fr 2fr;"
					>
						<span class="text-[var(--text-strong)]">JetBrains Mono Variable</span>
						<code class="text-[var(--interactive-active-fg)]">body default</code>
						<code class="text-[var(--interactive-active-fg)]">font-mono</code>
						<span>Numbers, timestamps, HTTP status, JSON, metrics, tool calls, ALL data</span>
					</div>

					<div
						class="grid gap-3 text-xs text-[var(--text-2)] py-1 border-b border-[var(--border-subtle)]"
						style="grid-template-columns: 1fr 1fr 1fr 2fr;"
					>
						<span class="text-[var(--text-strong)]">Syne Variable</span>
						<code class="text-[var(--interactive-active-fg)]">var(--font-display)</code>
						<code class="text-[var(--interactive-active-fg)]">font-display</code>
						<span>Section titles, KPI category labels, large numeric values, column headers</span>
					</div>

					<div
						class="grid gap-3 text-xs text-[var(--text-2)] py-1 border-b border-[var(--border-subtle)]"
						style="grid-template-columns: 1fr 1fr 1fr 2fr;"
					>
						<span class="text-[var(--text-strong)]">Inter Variable</span>
						<code class="text-[var(--interactive-active-fg)]">var(--font-sans)</code>
						<code class="text-[var(--interactive-active-fg)]">font-sans</code>
						<span>Prose help text, onboarding copy, settings panels, non-data descriptions</span>
					</div>

					<div
						class="grid gap-3 text-xs text-[var(--text-2)] py-1 border-b border-[var(--border-subtle)]"
						style="grid-template-columns: 1fr 1fr 1fr 2fr;"
					>
						<span class="text-[var(--text-strong)]">Geist Mono Variable</span>
						<code class="text-[var(--interactive-active-fg)]">var(--font-mono-alt)</code>
						<code class="text-[var(--interactive-active-fg)]">font-mono-alt</code>
						<span>Code panels, diff views, dense data grids (more neutral than JetBrains)</span>
					</div>

					<div
						class="grid gap-3 text-xs text-[var(--text-2)] py-1"
						style="grid-template-columns: 1fr 1fr 1fr 2fr;"
					>
						<span class="text-[var(--text-faint)]">Iosevka Term</span>
						<code class="text-[var(--text-3)]">var(--font-mono-condensed)</code>
						<code class="text-[var(--text-3)]">—</code>
						<span class="text-[var(--text-3)]"
							>Extremely column-dense views (system-only, degrades to JetBrains)</span
						>
					</div>
				</div>
			</div>

			<div class="card-surface p-5 flex flex-col gap-3">
				<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">The 3-second rule</p>
				<div class="flex flex-col gap-2">
					<p class="text-xs text-[var(--text-1)]">
						1. Is it a number, timestamp, path, or anything you'd read as "data"? → <span
							class="text-[var(--interactive-active-fg)]">JetBrains Mono</span
						> (default, no class needed)
					</p>
					<p class="text-xs text-[var(--text-1)]">
						2. Is it a heading, KPI label, or large numeric display value? → <span
							class="text-[var(--interactive-active-fg)]">font-display</span
						> (Syne)
					</p>
					<p class="text-xs text-[var(--text-1)]">
						3. Is it prose someone needs to read comfortably? → <span
							class="text-[var(--interactive-active-fg)]">font-sans</span
						> (Inter)
					</p>
					<p class="text-xs text-[var(--text-1)]">
						4. Is it a code panel that needs to feel neutral? → <span
							class="text-[var(--interactive-active-fg)]">font-mono-alt</span
						> (Geist Mono)
					</p>
				</div>
			</div>
		</div>
	{/snippet}
</Story>
