<script lang="ts">
	const REMINDER_FORMAT = `<system-reminder source="proxy-lessons">
Lessons recorded from prior sessions:
- [correction] <full lesson text, un-truncated>
</system-reminder>`;
</script>

<section
	class="wtt-card"
	data-design="funk"
	data-augmented-ui="tl-clip br-clip border"
	aria-label="What memory consolidation touches"
>
	<p class="wtt-line">
		<strong>Your files are not altered.</strong> Not CLAUDE.md, not agent or skill files, not rules, not
		MEMORY.md. Memory consolidation never writes to your project — it only reads and reshapes rows in
		its own lesson store.
	</p>
	<p class="wtt-line">
		What it <em>does</em> do: on an eligible request, the text of your project's active lessons is added
		to the request before it is sent — appended to the end of your last message, inside a block like this,
		exactly as captured on the wire:
	</p>
	<div class="code-block">
		<div class="bar"><span class="lang">appended to last user message</span></div>
		<pre>{REMINDER_FORMAT}</pre>
	</div>
	<p class="wtt-line">
		Only <code>daily</code> and <code>long_term</code> lessons are eligible — scoped to this project,
		and capped by the max-lessons setting.
	</p>
</section>

<style>
	.wtt-card {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--edge-line);
		background: var(--edge-surface);
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--edge-pad, calc(var(--edge) + var(--space-1)));
	}

	.wtt-line {
		margin: 0;
		font-size: var(--text-xs);
		line-height: var(--leading-body);
		color: var(--text-2);
	}
	.wtt-line strong {
		color: var(--text-1);
	}
	.wtt-line code {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--text-1);
		background: var(--surface-2);
		border-radius: var(--r-tol);
		padding: 0 var(--space-1);
	}
</style>
