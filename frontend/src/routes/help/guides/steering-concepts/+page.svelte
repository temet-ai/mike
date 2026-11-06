<script lang="ts">
	import GuideLayout from '$lib/components/help/GuideLayout.svelte';
	import { resolve } from '$app/paths';

	const toc = [
		{ slug: 'what-is-steering', label: 'What is steering?' },
		{ slug: 'the-lifecycle', label: 'The lifecycle' },
		{ slug: 'authority', label: 'Authority levels' },
		{ slug: 'mode', label: 'Mode: augment vs drive' },
		{ slug: 'enrolment', label: 'Enrolment' },
		{ slug: 'getting-started-now', label: 'Getting started' }
	];
</script>

<GuideLayout
	title="How steering works"
	summary="Steering lets you attach autonomous correction agents to live sessions. This guide explains the lifecycle, authority levels, and mode options with concrete examples."
	audience="operator"
	duration="5 min"
	{toc}
>
	<h2 id="what-is-steering">What is steering?</h2>
	<p>
		Steering lets you point a live Claude Code session back at its task when it drifts. You write a
		small agent — a steerer — that watches the session's turns, detects when the work wanders off
		track, and sends a corrective message on your behalf. The session keeps running; the steerer
		rides alongside it.
	</p>
	<p>
		A steerer is a named configuration for that watching agent. It has a prompt that defines what
		"on track" means, a model that runs the drift detection, and a set of thresholds that control
		how sensitive and how forceful it is. One steerer can watch many sessions; one session can have
		many steerers.
	</p>

	<h2 id="the-lifecycle">The lifecycle</h2>
	<ol>
		<li>
			<strong>Enrol</strong> — you attach a steerer to a live session. The steerer now watches every new
			turn that session produces.
		</li>
		<li>
			<strong>Evaluate</strong> — after each turn completes, the steerer calls its configured model against
			the steerer's own prompt and gets back a typed verdict: whether to fire, a confidence score between
			0 and 1, a reason, and the nudge text to send.
		</li>
		<li>
			<strong>Decide</strong> — if confidence is at or above the steerer's confidence threshold, and the
			debounce window has passed, the steerer fires.
		</li>
		<li>
			<strong>Steer</strong> — the fired steerer's message is queued and delivered on the session's next
			request, appended into that turn as a labelled note. The session's Claude sees it alongside everything
			else and may adjust course.
		</li>
	</ol>
	<p>
		The steerer's model call is a short, cheap, structured-output completion — the fleet defaults to
		<code>google/gemini-3.5-flash-lite</code>, and any steerer can override the model it uses. It
		costs a fraction of a cent per turn and runs off the session's hot path: the response has
		already reached the client before the fleet evaluates it, so steering never adds latency to the
		turn it's evaluating.
	</p>

	<h2 id="authority">Authority levels</h2>
	<p>
		Authority is meant to describe how forceful a steered message should be — a field you set when
		creating a steerer, or override per enrolment. Three levels, weakest to strongest:
	</p>
	<div class="guide-table-frame" data-augmented-ui="tl-clip br-clip border">
		<table class="guide-table">
			<thead>
				<tr>
					<th>Level</th>
					<th>What it's meant to convey</th>
				</tr>
			</thead>
			<tbody>
				<tr>
					<td><code>supply_context</code></td>
					<td
						>Background information appears in the context. Claude may use it or ignore it. Example:
						"Note: this session's working task is X."</td
					>
				</tr>
				<tr>
					<td><code>redirect</code></td>
					<td
						>A suggestion to change direction. Phrased as a request; Claude is free to push back.
						Example: "You appear to be refactoring the wrong file — please return to main.rs."</td
					>
				</tr>
				<tr>
					<td><code>command</code></td>
					<td
						>Meant to read as a binding directive. Example: "Stop. The current approach will break
						the migration. Revert and follow the spec at docs/MIGRATION_PLAN.md."</td
					>
				</tr>
			</tbody>
		</table>
	</div>
	<div class="callout" data-augmented-ui="tl-clip br-clip border">
		<span class="callout-title">Authority doesn't change how the message is delivered</span>
		<span>
			Every fired steerer's message is delivered the same way today, regardless of authority:
			wrapped as a labelled note ("Steering note from the operator's proxy: [the steerer's message].
			If it is already done or does not apply, say so in a line.") and appended into the session's next turn.
			Authority is recorded with the intervention for review, and it's what the guided create flow's
			AI assist uses to decide how forceful the steerer's own prompt should read — treat the three
			levels as instructions to yourself when writing that prompt, not as a guarantee the session
			will honour a
			<code>command</code> steer any differently than a <code>supply_context</code> one.
		</span>
	</div>
	<p>
		Start with <code>supply_context</code>. Reserve <code>redirect</code> for when context additions
		consistently fail. Reserve <code>command</code> for well-tested, trusted steerers, and write its prompt
		as plainly and forcefully as you'd want an instruction read.
	</p>

	<h2 id="mode">Mode: augment vs drive</h2>
	<p>
		Mode is the other field a steerer carries, meant to distinguish a light nudge from a stronger
		intervention:
	</p>
	<div class="guide-table-frame" data-augmented-ui="tl-clip br-clip border">
		<table class="guide-table">
			<thead>
				<tr>
					<th>Mode</th>
					<th>What it's meant to convey</th>
				</tr>
			</thead>
			<tbody>
				<tr>
					<td><code>augment</code></td>
					<td
						>The steering message is added alongside the session's existing context. The session's
						Claude still generates the next turn naturally.</td
					>
				</tr>
				<tr>
					<td><code>drive</code></td>
					<td>Meant as the more invasive option — a stronger intervention than augment.</td>
				</tr>
			</tbody>
		</table>
	</div>
	<div class="callout" data-augmented-ui="tl-clip br-clip border">
		<span class="callout-title">Drive doesn't replace the session's turn</span>
		<span>
			Today, a fired steerer's message is always appended alongside the session's next turn, never
			substituted for it — the same delivery <code>augment</code> describes. The steerer fleet
			evaluates after a response has already reached the client (it runs off the session's hot
			path), so there's no point in the pipeline where a steerer could stand in for that turn's
			response instead of the model. Set <code>drive</code> to mark an intervention as more serious for
			anyone reviewing it later; a different mechanism entirely, the response-side weasel intercept, is
			what can replace a response, and it isn't part of steering.
		</span>
	</div>
	<p>
		For day-to-day steering, <code>augment</code> + <code>supply_context</code> is the right combination
		— it's also how every steer is delivered today, regardless of which mode you pick.
	</p>

	<h2 id="enrolment">Enrolment</h2>
	<p>
		Enrolment is the link between a session and a steerer. One enrolment = one steerer watching one
		session. You can enrol multiple steerers for the same session (they run in priority order), or
		enrol the same steerer in multiple sessions. Disenrolling removes the link immediately — the
		steerer stops watching that session at the next turn evaluation.
	</p>
	<p>
		You can enrol a session before or after creating the steerer. The session detail page (inside
		any active session) has a "Steering activity" section with an enrol button. Alternatively, the
		guided create flow lets you enrol the target session as the final step of creating a new
		steerer.
	</p>

	<h2 id="getting-started-now">Getting started</h2>
	<p>The fastest path:</p>
	<ol>
		<li>Go to <a href={resolve('/steering?tab=fleet')}>Steering → Fleet → New steerer</a>.</li>
		<li>Pick a live session you want to watch.</li>
		<li>Describe what the session should be focused on in one sentence.</li>
		<li>Choose <code>supply_context</code> + <code>augment</code> (the safe defaults).</li>
		<li>Create the steerer — it enrols the session automatically.</li>
	</ol>
	<p>
		You will see events appear in the <a href={resolve('/steering?tab=monitor')}>Monitor tab</a> as soon
		as the session drifts.
	</p>
	<div class="callout" data-augmented-ui="tl-clip br-clip border">
		<span class="callout-title">Start conservative</span>
		<span>
			Use <code>supply_context</code> + <code>augment</code> until you have verified the steerer's drift
			detection is accurate. Move to a stronger authority label only after watching several correct fires
			in the Monitor tab.
		</span>
	</div>
</GuideLayout>

<style>
	.guide-table-frame {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-warning-fg) 0%,
			var(--status-warning-border) 45%,
			var(--border-default) 80%
		);
		padding: calc(var(--edge) + 4px);
		background: var(--surface-card);
		overflow-x: auto;
	}
	.guide-table {
		width: 100%;
		min-width: 22rem;
		border-collapse: collapse;
		font-size: 0.8125rem;
		color: var(--text-1);
	}
	.guide-table th {
		text-align: left;
		padding: 7px 12px;
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
	}
	.guide-table td {
		padding: 9px 12px;
		border-bottom: 1px solid var(--border-subtle);
		line-height: 1.55;
		vertical-align: top;
	}
	.guide-table td:first-child {
		white-space: nowrap;
		color: var(--text-strong);
	}
	.guide-table tr:last-child td {
		border-bottom: none;
	}
</style>
