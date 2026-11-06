<script lang="ts">
	import GuideLayout from '$lib/components/help/GuideLayout.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { resolve } from '$app/paths';

	const toc = [
		{ slug: 'getting-there', label: 'Open a session' },
		{ slug: 'session-header', label: 'Session header' },
		{ slug: 'the-timeline', label: 'The turn timeline' },
		{ slug: 'request-drawer', label: 'The request drawer' },
		{ slug: 'finding-things', label: 'Finding things fast' }
	];

	interface DrawerBandKey {
		request: boolean;
		response: boolean;
		events: boolean;
		findings: boolean;
	}
	let drawerBandsOpen = $state<DrawerBandKey>({
		request: true,
		response: true,
		events: true,
		findings: true
	});
	function toggleDrawerBand(key: keyof DrawerBandKey): void {
		drawerBandsOpen[key] = !drawerBandsOpen[key];
	}
</script>

<GuideLayout
	title="Inspect a captured request"
	summary="A session's captured traffic renders as a scrolling timeline of turns. This guide walks that layout — what a turn's header and body show, where the system prompt appears, and where the full request and response bytes live when a turn only shows a summary."
	audience="operator"
	duration="4 min"
	{toc}
>
	<h2 id="getting-there">Open a session</h2>
	<p>
		Pick a session from the left column. Its detail opens with a session identity bar at the top
		and, below it, the session's <em>turn timeline</em> — the surface this guide is about. Clicking a
		specific request (from search, or a link elsewhere) deep-links into the same timeline, scrolled to
		that request's turn.
	</p>

	<h2 id="session-header">Session header</h2>
	<p>
		This is the request-detail view's own header — a different, richer bar than the plain session
		overview's, which shows only identity chrome and keeps these figures in its own metrics card
		further down the page. Here, the bar across the top of the detail column shows, left to right:
	</p>
	<ul>
		<li>
			The <a href={resolve('/help/glossary#session')}>session id</a>, linking back to the session
			overview, and its title when one is set.
		</li>
		<li>Total requests captured under this session.</li>
		<li>When the session started, and the span it covers once it has run a while.</li>
		<li>
			The distinct <a href={resolve('/help/glossary#model-id')}>models</a> used, when a session mixes
			more than one.
		</li>
	</ul>
	<p>
		These are session-level aggregates, not per-request figures — a specific request's own method,
		status and token counts show on its own turn in the timeline below, not up here.
	</p>

	<h2 id="the-timeline">The turn timeline</h2>
	<p>
		Below the header, the session's operator turns render as a continuous scrolling list, newest at
		the bottom. Each turn is a band with its own header row — click the row to expand or collapse
		it, the same disclosure pattern used throughout this app. A turn's header shows a turn number,
		an outcome badge, a preview of the prompt, elapsed time, input/output token counts, and a <strong
			>view request</strong
		> link.
	</p>
	<p>
		Expanding a turn reveals its body: <strong>Reasoning</strong> (extended thinking, when the model
		used it), <strong>Tools</strong> (each
		<a href={resolve('/help/glossary#tool-call')}>tool call</a> paired with its
		<a href={resolve('/help/glossary#tool-result')}>result</a>), then the
		<strong>Reply</strong>.
	</p>
	<p>
		The <a href={resolve('/help/glossary#system-block')}>system prompt</a> renders inline in the
		timeline as its own card, not per turn — once, at the point it's relevant. Its blocks are
		grouped by what they are (billing metadata, the identity line, and the larger instruction
		blocks) rather than by position, and a block the proxy rewrote before sending carries a badge
		and expands to a side-by-side original/sent
		<a href={resolve('/help/glossary#composition')}>diff</a>, so you can see exactly where a
		captured prompt was altered on the wire.
	</p>

	<h2 id="request-drawer">The request drawer</h2>
	<p>
		Click <strong>view request</strong> on any turn to open the drawer — a focused view for the bytes
		that don't belong inline in the timeline. It carries four kinds:
	</p>
	<div class="edge mech-group" data-augmented-ui="tl-clip br-clip border">
		<div>
			<div class="band-hd-wrap">
				<button
					type="button"
					class="band-hd"
					aria-expanded={drawerBandsOpen.request}
					onclick={() => toggleDrawerBand('request')}
				>
					<DisclosureMarker open={drawerBandsOpen.request} />
					<span>Request body</span>
					<span class="band-hd__meta">original · sent · transformations / diff</span>
				</button>
			</div>
			{#if drawerBandsOpen.request}
				<div class="band-body">
					<p>
						What was actually sent to Anthropic, plus the original body before any filter rule
						touched it. The <strong>transformations</strong> and
						<strong>diff</strong> views are where a rewritten request shows exactly what changed and why.
					</p>
				</div>
			{/if}
		</div>
		<div>
			<div class="band-hd-wrap">
				<button
					type="button"
					class="band-hd"
					aria-expanded={drawerBandsOpen.response}
					onclick={() => toggleDrawerBand('response')}
				>
					<DisclosureMarker open={drawerBandsOpen.response} />
					<span>Response body</span>
				</button>
			</div>
			{#if drawerBandsOpen.response}
				<div class="band-body">
					<p>The accumulated response exactly as Anthropic returned it.</p>
				</div>
			{/if}
		</div>
		<div>
			<div class="band-hd-wrap">
				<button
					type="button"
					class="band-hd"
					aria-expanded={drawerBandsOpen.events}
					onclick={() => toggleDrawerBand('events')}
				>
					<DisclosureMarker open={drawerBandsOpen.events} />
					<span>SSE events</span>
				</button>
			</div>
			{#if drawerBandsOpen.events}
				<div class="band-body">
					<p>
						Every <a href={resolve('/help/glossary#sse-accumulator')}>event / data pair</a> the upstream
						emitted, captured verbatim. Use this to diagnose mid-stream drops or to compare streamed vs.
						accumulated text.
					</p>
				</div>
			{/if}
		</div>
		<div>
			<div class="band-hd-wrap">
				<button
					type="button"
					class="band-hd"
					aria-expanded={drawerBandsOpen.findings}
					onclick={() => toggleDrawerBand('findings')}
				>
					<DisclosureMarker open={drawerBandsOpen.findings} />
					<span>Findings</span>
				</button>
			</div>
			{#if drawerBandsOpen.findings}
				<div class="band-body">
					<p>
						<a href={resolve('/help/glossary#weasel-pattern')}>Weasel pattern</a>
						findings against the response body. Diagnostic, never blocking — see the
						<a href={resolve('/help/guides/anomalies')}>anomalies guide</a> for one workflow that uses
						these.
					</p>
				</div>
			{/if}
		</div>
	</div>

	<h2 id="finding-things">Finding things fast</h2>
	<ul>
		<li>
			<strong>Hover any cell</strong> for a tooltip; click any
			<a href={resolve('/help/glossary')}>glossary</a> term to open the definition.
		</li>
		<li>
			<strong>Cmd-K</strong> opens the command palette — jump to a session, request, or composition by
			id.
		</li>
		<li>
			<strong>The full request and response body</strong> is always one click away under the request
			drawer. The chrome never amputates inspection content (<code
				>docs/.claude/rules/agent-behavior-and-data.md</code
			>).
		</li>
	</ul>
</GuideLayout>
