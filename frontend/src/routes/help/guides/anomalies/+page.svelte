<script lang="ts">
	import GuideLayout from '$lib/components/help/GuideLayout.svelte';
	import { resolve } from '$app/paths';

	const toc = [
		{ slug: 'what-is-an-anomaly', label: 'What is an anomaly' },
		{ slug: 'open-the-catalogue', label: 'Open the catalogue' },
		{ slug: 'read-a-row', label: 'Read a row' },
		{ slug: 'act-on-it', label: 'Act on it' }
	];
</script>

<GuideLayout
	title="Investigate a prompt anomaly"
	summary="Anomalies are the honest 'I don't recognise this' signal — system-prompt shapes that didn't classify cleanly. They never block requests; they record divergence so you can decide what to do."
	audience="operator"
	duration="4 min"
	{toc}
>
	<h2 id="what-is-an-anomaly">What is an anomaly</h2>
	<p>
		An <a href={resolve('/help/glossary#anomaly')}>anomaly</a> is a captured system prompt whose
		shape didn't classify cleanly against the registered
		<a href={resolve('/help/glossary#composition')}>compositions</a>. Two failure modes are
		recorded:
	</p>
	<ul>
		<li>
			<strong><code>unknown-class</code></strong> — a brand-new shape never seen before.
		</li>
		<li>
			<strong><code>structural-mismatch</code></strong> — resembles a known composition but differs in
			detail (block count, block order, hash drift on individual blocks).
		</li>
	</ul>
	<p>
		Within those, an <a href={resolve('/help/glossary#anomaly-sub-pattern')}>anomaly sub-pattern</a>
		gives a more specific classification — for example
		<code>unregistered-subagent</code> when a recognised subagent persona shows up with no composition
		registered for its shape yet — and a recommended action.
	</p>

	<h2 id="open-the-catalogue">Open the catalogue</h2>
	<p>
		Navigate to <a href={resolve('/prompts')}>/prompts</a>. Every prompt class the proxy has ever
		seen lives in one table — registered or not — so anomalies sit beside everything else rather
		than in a separate list. Filter to <strong>Needs investigation</strong> for structural-mismatch
		anomalies (a captured shape that resembles a known class but differs in detail) or
		<strong>Unregistered</strong> for unknown-class ones. Rows are grouped by state — drifted and needs-investigation
		first — and ordered newest-first within each group.
	</p>

	<h2 id="read-a-row">Read a row</h2>
	<p>The whole row is one link. It shows:</p>
	<ul>
		<li>
			A recognised class name, or — when the shape doesn't resolve to a stable identity — a warning
			icon and a plain-English description of what's wrong (e.g. "system had no <code>text</code>
			field").
			<strong>The description is the analyzer's full text</strong> — no truncation, no slicing.
		</li>
		<li>A state badge (e.g. <code>needs investigation</code>, <code>unregistered</code>).</li>
		<li>
			A wrench icon, shown only when an existing filter rule already rewrites this shape — hover it
			for the rule count.
		</li>
		<li>The observation count and when it was last seen.</li>
		<li>One action on the right, telling you exactly what to do next — see below.</li>
	</ul>

	<h2 id="act-on-it">Act on it</h2>
	<p>
		The action a row offers is chosen by the specific
		<a href={resolve('/help/glossary#anomaly-sub-pattern')}>sub-pattern</a> the analyzer recognised, not
		a generic "handle this anomaly" button:
	</p>
	<ul>
		<li>
			<strong>Register this</strong> — the common case (e.g.
			<code>unregistered-class</code>, or <code>unregistered-subagent</code>
			when a known subagent persona shows up with no composition of its own). Opens
			<a href={resolve('/prompts/new')}>/prompts/new</a>, prefilled from the most recent
			observation, where you register a new composition or a variant of an existing one. The next
			matching request classifies cleanly instead of firing another anomaly.
		</li>
		<li>
			<strong>View filter diff</strong> — appears only when a filter rule you already wrote is the reason
			this shape looks anomalous (the composer is reacting to your own filter, not to a missing registration).
			Jumps straight to that request's transformation diff so you can confirm the rewrite did what you
			intended.
		</li>
		<li>
			<strong>Investigate</strong> — appears when the shape needs a human look before any
			registration makes sense (for example, a caller sent no
			<code>system</code> field at all). Opens the raw request.
		</li>
	</ul>
	<p>
		None of these touch outbound traffic on their own. Registering a composition changes how future
		requests <em>classify</em>; only a
		<a href={resolve('/help/guides/filter-rules')}>filter rule</a> rewrites outbound bytes.
	</p>
	<div class="callout" data-augmented-ui="tl-clip br-clip border">
		<span class="callout-title">Anomalies never block requests</span>
		<span>
			An anomaly is a record of divergence, not a gate. Requests reach Anthropic regardless of how
			they classify.
		</span>
	</div>
</GuideLayout>
