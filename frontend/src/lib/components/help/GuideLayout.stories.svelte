<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import GuideLayout from './GuideLayout.svelte';

	const TOC_ITEMS = [
		{ slug: 'overview', label: 'Overview' },
		{ slug: 'quick-start', label: 'Quick start' },
		{ slug: 'configuration', label: 'Configuration' },
		{ slug: 'advanced', label: 'Advanced usage' },
		{ slug: 'troubleshooting', label: 'Troubleshooting' }
	];

	const { Story } = defineMeta({
		title: 'Routes/Help/GuideLayout',
		component: GuideLayout,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Shared chrome for /help/guides/<slug> pages. Owns: page head (title, audience + duration pills, summary), optional sticky table-of-contents column, and "Back to guides" footer link.'
				}
			}
		}
	});
</script>

<Story name="With table of contents">
	{#snippet template(args)}
		<div class="bg-[var(--surface-bg)] min-h-[500px]">
			<GuideLayout
				title="Setting up the Claude proxy"
				summary="Step-by-step guide to installing, configuring, and running the Claude proxy in development."
				audience="Developer"
				duration="10 min"
				toc={TOC_ITEMS}
			>
				<section id="overview">
					<h2>Overview</h2>
					<p>
						The Claude proxy sits between Claude Code and the Anthropic API, capturing all request
						and response traffic for inspection.
					</p>
				</section>
				<section id="quick-start">
					<h2>Quick start</h2>
					<p>Clone the repository and run the start script:</p>
					<pre><code>./scripts/start-proxy.sh</code></pre>
					<p>Then point Claude Code at the proxy:</p>
					<pre><code>ANTHROPIC_BASE_URL=http://localhost:9988 claude</code></pre>
				</section>
				<section id="configuration">
					<h2>Configuration</h2>
					<p>
						Use <code>/settings</code> for runtime configuration. Boot defaults come from the
						installer-written <code>.env</code> bootstrap file and command-line flags.
					</p>
					<div class="callout" data-augmented-ui="tl-clip br-clip border">
						<span class="callout-title">Note</span>
						Configuration precedence: defaults &lt; <code>MIKE_*</code> env vars &lt; CLI flags.
					</div>
				</section>
				<section id="advanced">
					<h2>Advanced usage</h2>
					<p>
						The proxy exposes three ports: the proxy bind on 9988, the inspect API on 9989, and the
						SPA host on 9990.
					</p>
					<ul>
						<li>Use the inspect API to query captured requests programmatically</li>
						<li>The MCP bridge connects Claude Code to the inspect API directly</li>
						<li>Filter rules can rewrite or drop specific system prompt content</li>
					</ul>
				</section>
				<section id="troubleshooting">
					<h2>Troubleshooting</h2>
					<p>
						If the proxy is not capturing traffic, verify <code>ANTHROPIC_BASE_URL</code> is set correctly
						and the proxy is running on port 9988.
					</p>
				</section>
			</GuideLayout>
		</div>
	{/snippet}
</Story>

<Story name="Without table of contents">
	{#snippet template(args)}
		<div class="bg-[var(--surface-bg)] min-h-[400px]">
			<GuideLayout
				title="Reading captured requests"
				summary="How to use the inspect API and web UI to view and analyse captured Claude API traffic."
				audience="Operator"
				duration="5 min"
			>
				<section>
					<h2>Finding requests</h2>
					<p>
						Open the web UI at <code>http://localhost:9990</code>. The left column lists all
						captured sessions; clicking a session expands its requests.
					</p>
				</section>
				<section>
					<h2>Inspecting a request</h2>
					<p>
						Click any request row to open the detail pane. The detail pane shows the full request
						body, response body, and all headers — no content is truncated.
					</p>
				</section>
			</GuideLayout>
		</div>
	{/snippet}
</Story>

<Story name="Long content (full render, no clamp)">
	{#snippet template(args)}
		<div class="bg-[var(--surface-bg)] min-h-[400px]">
			<GuideLayout
				title="Understanding the session memory model"
				summary="A deep-dive into how the proxy stores, evicts, and rehydrates session data across restarts."
				audience="Operator"
				duration="15 min"
			>
				<section>
					<h2>Session lifecycle</h2>
					<p>
						Every Claude Code session creates a session row in the proxy's in-memory store. The
						session accumulates requests until the TTL expires or the proxy is restarted.
					</p>
					<p>
						On restart, the proxy rehydrates sessions from PostgreSQL (when persistence is enabled).
						Requests are replayed in chronological order, preserving the full message history.
					</p>
					<p>
						Sessions are evicted from the in-memory store after the configured TTL (default 24
						hours). Pinned sessions are exempt from TTL eviction.
					</p>
				</section>
				<section>
					<h2>Per-project TTL override</h2>
					<p>You can set a per-project TTL via the inspect API or the MCP bridge:</p>
					<pre><code
							>PATCH /api/projects/&lt;path&gt;/ttl
&#123;"ttl_secs": 86400&#125;</code
						></pre>
					<p>
						This overrides the global default for all sessions under that project path. Set <code
							>ttl_secs: null</code
						> to clear the override and revert to the global default.
					</p>
				</section>
				<section>
					<h2>Pinning sessions</h2>
					<p>
						Pinned sessions never expire, regardless of TTL. Pin a session when you need to keep it
						available indefinitely for debugging or reference.
					</p>
					<p>
						Pinning is also available at the project level — a project pin exempts all sessions
						under that project from TTL eviction.
					</p>
					<div class="callout" data-augmented-ui="tl-clip br-clip border">
						<span class="callout-title">Warning</span>
						Pinned sessions accumulate indefinitely. Monitor your database size if you pin many sessions.
						Unpin them once you no longer need them.
					</div>
				</section>
			</GuideLayout>
		</div>
	{/snippet}
</Story>
