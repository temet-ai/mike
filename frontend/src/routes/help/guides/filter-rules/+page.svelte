<script lang="ts">
	import GuideLayout from '$lib/components/help/GuideLayout.svelte';
	import { resolve } from '$app/paths';

	const toc = [
		{ slug: 'when-to-use', label: 'When to use a filter' },
		{ slug: 'open-the-workbench', label: 'Open the workbench' },
		{ slug: 'create-a-rule', label: 'Create a rule' },
		{ slug: 'order-and-overlap', label: 'Order & overlap' }
	];
</script>

<GuideLayout
	title="Rewrite outbound system prompts"
	summary="Filter rules are mike's one deliberate exception to byte-identity forwarding — they let you drop, replace, or append text in the outbound system prompt and message content before it reaches Anthropic."
	audience="operator"
	duration="5 min"
	{toc}
>
	<h2 id="when-to-use">When to use a filter</h2>
	<p>
		<a href={resolve('/help/glossary#filter-rule')}>Filter rules</a> exist for one job: rewriting
		text blocks in the outbound <code>system</code> array and message content of
		<code>/v1/messages</code> before it reaches Anthropic. Use them when you want to:
	</p>
	<ul>
		<li>
			<strong>Strip operator-specific reminders</strong> (e.g. project-specific instructions in
			<code>CLAUDE.md</code>) from a known wrapper like
			<code>&lt;system-reminder&gt;</code>.
		</li>
		<li>
			<strong>Redact a literal string</strong> (an internal URL, a personal name) without dropping the
			surrounding context.
		</li>
		<li>
			<strong>Append a marker</strong> after a known block — e.g. appending an operator instruction after
			Claude Code's preamble.
		</li>
	</ul>
	<div class="callout" data-augmented-ui="tl-clip br-clip border">
		<span class="callout-title">What filters do and don't touch</span>
		<span>
			Filters rewrite <code>{'{type: "text"}'}</code> blocks — in both the
			<code>system</code> array and every message's <code>content</code> array, not the
			<code>system</code>
			field alone. Tool calls, tool results, and any other non-text block always pass through untouched,
			and so does the response side entirely — filters only ever act on the outbound request. A string-shaped
			<code>system</code>
			field is left alone too. See
			<a href={resolve('/help/glossary#byte-identity-forwarding')}>byte-identity forwarding</a> for
			what "outbound request" means here. One asymmetry worth knowing: blocks in <code>system</code> before
			the last global cache-control boundary are protected from rules; message content carries no such
			protection.
		</span>
	</div>
	<div class="callout" data-augmented-ui="tl-clip br-clip border">
		<span class="callout-title">A filter can break extended thinking</span>
		<span>
			Anthropic verifies a signature on <code>thinking</code> blocks against the original request bytes.
			Any rule that fires against a request whose message content holds a thinking block invalidates that
			signature and gets a 400 back from Anthropic. If you use extended thinking, check the per-block
			badge in the workbench before enabling a rule — it shows which rules would fire against a given
			request.
		</span>
	</div>

	<h2 id="open-the-workbench">Open the workbench</h2>
	<p>
		Filter rules are authored on a dedicated page, the <strong>filter workbench</strong>
		(<code>/filter/workbench</code>) — there is no drawer. Get there one of two ways:
	</p>
	<ul>
		<li>
			<strong>From anywhere</strong> — click <strong>Filters</strong> in the left navigation (under
			Levers), or open the command palette and run
			<strong>Open filter workbench</strong>. This lands on a catalogue of every captured
			system-prompt class, each with an <strong>Open in workbench</strong>
			link into its most recently captured prompt.
		</li>
		<li>
			<strong>From a specific prompt</strong> — on a request's system prompt (in the session view),
			hover any block and click the filter icon (<strong>Filter this block</strong>). This opens the
			workbench with that request's system prompt already loaded as the source.
		</li>
	</ul>
	<p>
		Once a prompt is loaded, the workbench is three panes side by side: the source system prompt on
		the left (full text, split into blocks), the ordered rule stack in the middle, and a live
		preview of the filtered result on the right — each rule's <a
			href={resolve('/help/glossary#filter-rule-kind')}>kind</a
		>,
		<a href={resolve('/help/glossary#filter-rule-action')}>action</a>, value, and how recently it
		has fired against real traffic show on its card in the middle pane.
	</p>

	<h2 id="create-a-rule">Create a rule</h2>
	<p>There are two ways to start a rule, and both land on the same form:</p>
	<ul>
		<li>
			<strong>Select text in the source pane.</strong> Highlight any span of the loaded system
			prompt and a popover offers <strong>Drop slice</strong>,
			<strong>Replace with…</strong>, or <strong>Insert after…</strong> — pick one and the form opens
			pre-filled with the selected text and that action.
		</li>
		<li>
			Click the <strong>Add rule</strong> button at the top of the rule-stack pane for a blank form.
		</li>
	</ul>
	<p>A rule has three required fields:</p>
	<ol>
		<li>
			<strong>Match shape</strong> — how the value matches. The two common shapes:
			<ul>
				<li>
					<a href={resolve('/help/glossary#kind-text-substring')}><code>text_substring</code></a>:
					literal case-sensitive substring match anywhere in a block.
				</li>
				<li>
					<a href={resolve('/help/glossary#kind-xml-tag')}><code>xml_tag</code></a>: match a
					<code>&lt;tag&gt;…&lt;/tag&gt;</code>
					element by tag name (no angle brackets in the value). Convenient for the
					<code>&lt;system-reminder&gt;</code> wrapper.
				</li>
			</ul>
			The form offers two further, more specialized shapes:<strong>lazy section</strong>
			matches a heading line and lazily swaps the section below it for a stub until the model emits a
			trigger token, and <strong>inject reminder</strong> injects a block of text once per context window
			when the model emits a trigger marker — neither matches or rewrites existing content the way the
			two shapes above do.
		</li>
		<li>
			<strong>Value</strong> — the substring or tag name to match. It defaults to the full text of
			the block you selected from; for
			<code>text_substring</code>, a <strong>Paragraph</strong> or <strong>Line</strong>
			preset narrows it to just the first paragraph or line instead of hand-editing it down.
		</li>
		<li>
			<strong>Action</strong> — what to do on match.
			<ul>
				<li>
					<a href={resolve('/help/glossary#action-drop')}><code>drop</code></a>: remove the matched
					span. The Anthropic-bound bytes don't include it.
				</li>
				<li>
					<a href={resolve('/help/glossary#action-replace')}><code>replace</code></a>: substitute
					the rule's <code>text</code> payload for the matched span. The <code>text</code> field appears
					once you pick this action.
				</li>
				<li>
					<a href={resolve('/help/glossary#action-append')}><code>append</code></a>: insert the
					<code>text</code> payload after the matched span, leaving the original in place.
				</li>
			</ul>
		</li>
	</ol>
	<p>
		Submitting adds the rule to the stack in the middle pane. If a prompt is loaded as source, the
		preview pane on the right updates to show the filtered result.
	</p>

	<h2 id="order-and-overlap">Order & overlap</h2>
	<p>
		Rules apply <strong>top to bottom</strong>. The first matching rule for a given match position
		wins; subsequent rules see the rewritten text. Drag the grip handle at the left of a rule's card
		to reorder, or focus it and press the up/down arrow keys.
	</p>
	<p>
		Disable rather than delete when you're investigating: the
		<strong>Enabled</strong> toggle keeps the rule in the list but skips it during matching.
	</p>
</GuideLayout>
