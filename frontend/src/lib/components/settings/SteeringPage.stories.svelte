<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import WorkerCard from './WorkerCard.svelte';
	import type { WorkerModel } from './WorkerCard.svelte';
	import DestinationNav from '$lib/components/shell/DestinationNav.svelte';

	const STEERER_MODELS: WorkerModel[] = [
		{
			id: 'openai/gpt-5.6-luna',
			released: 'Jul 2024',
			pricing: '$0.15 / $0.60 per 1M',
			context: '128K ctx',
			description:
				"OpenAI's small, low-cost chat model. Fast and cheap — a strong default for steerer verdicts, which are short structured judgments where latency and price matter more than deep reasoning."
		},
		{
			id: 'google/gemini-3.5-flash-lite',
			released: 'Jun 2025',
			pricing: '$0.10 / $0.40 per 1M',
			context: '1M ctx',
			description:
				"Google's lightest Gemini 2.5 tier. The cheapest option here, with a huge context window — useful when a verdict must weigh a lot of prior turns at once."
		},
		{
			id: 'google/gemini-3.6-flash',
			released: 'Jun 2025',
			pricing: '$0.30 / $2.50 per 1M',
			context: '1M ctx',
			description:
				"Google's mid Gemini 2.5 tier. Stronger judgment than flash-lite at a higher price; a good middle ground when verdict quality matters."
		},
		{
			id: 'google/gemini-3.8-flash',
			released: 'Oct 2025',
			pricing: '$1.00 / $5.00 per 1M',
			context: '200K ctx',
			description:
				"Anthropic's fast Haiku 4.5. Markedly better at instruction-following and nuanced judgment than the mini tiers — worth the price when the steerer's calls need to be reliable."
		},
		{
			id: 'openai/gpt-5.4-mini',
			released: 'May 2024',
			pricing: '$2.50 / $10.00 per 1M',
			context: '128K ctx',
			description:
				"OpenAI's full 4o model. The most capable (and most expensive) here; reserve it for steerers where verdict accuracy is critical and volume is low."
		}
	];
	const INHERIT_MODEL: WorkerModel = {
		id: '',
		label: 'Inherit default model',
		description: "Use the steerer's default model (set above) for this sub-judge."
	};
	const SUBJUDGE_MODELS: WorkerModel[] = [INHERIT_MODEL, ...STEERER_MODELS];

	const STEERER_CONFIG = [
		{
			label: 'Request timeout',
			value: '30s',
			help: 'How long the proxy waits for a steerer verdict before giving up on that turn. Too low and slower models time out; 30s is comfortable for the models here.'
		},
		{
			label: 'Quiet period',
			value: '120s',
			help: "Minimum time between steerer injections in the same session, so a drifting session isn't nudged on every turn. 120s ≈ a couple of turns of breathing room."
		}
	];
	const JUDGE_CONFIG = [
		{
			label: 'Confidence threshold',
			value: '0.70',
			help: 'The verdict confidence (0–1) above which this sub-judge fires. Higher = fewer, more certain interventions; lower = more eager nudging. 0.70 fires only when the judge is fairly sure.'
		},
		{
			label: 'Debounce turns',
			value: '2',
			help: 'How many turns must pass after this sub-judge fires before it can fire again — prevents repeat nudges back-to-back.'
		}
	];
	const CORRECTION_CONFIG = [
		{
			label: 'Confidence threshold',
			value: '0.75',
			help: 'The confidence above which a correction is classified as a real operator correction worth learning from.'
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Settings/Steering',
		component: WorkerCard,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});
</script>

<script lang="ts">
	let steererOn = $state(true);
	let steererModel = $state('openai/gpt-5.6-luna');
	let refocusOn = $state(true);
	let refocusModel = $state('');
	let verifierOn = $state(true);
	let verifierModel = $state('');
	let correctionOn = $state(false);
	let correctionModel = $state('');
</script>

<Story
	name="Page"
	parameters={{
		sveltekit_experimental: {
			state: { page: { url: new URL('http://localhost/settings/steering'), params: {} } }
		}
	}}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.querySelector('a[href="/settings/steering"]')).not.toBeNull();
		await expect(canvasElement.querySelectorAll('.dsc-card').length).toBe(4);
	}}
>
	{#snippet template()}
		<div class="shell">
			<div class="nav"><DestinationNav /></div>
			<div class="content">
				<header class="st-head">
					<h1 class="st-title">Steering</h1>
					<p class="st-sub">
						The steerer fleet evaluates every turn of enrolled sessions and nudges them back on
						track when they drift. Configure the default model and the three sub-judges below.
					</p>
				</header>

				<div class="env">
					<div class="env-status">
						<span class="env-dot env-dot--ok" aria-hidden="true"></span>
						<span class="env-label"
							>OpenRouter API key: <span class="env-ok"
								>configured via MIKE_STEERER_OPENROUTER_API_KEY</span
							></span
						>
					</div>
					<div class="env-row">
						<span class="env-strong">Env var holding the OpenRouter API key</span>
						<div class="env-controls">
							<input
								class="env-input"
								aria-label="Env var holding the OpenRouter API key"
								value="MIKE_STEERER_OPENROUTER_API_KEY"
								readonly
							/>
							<button type="button" class="env-btn">Save &amp; Verify</button>
						</div>
					</div>
				</div>

				<WorkerCard
					name="steerer"
					label="Steerer"
					description="Evaluates every turn of enrolled sessions against configured drive topics and nudges the session back on track when it drifts. Requires the verified OpenRouter key above."
					enabled={steererOn}
					onToggle={(v) => (steererOn = v)}
					models={STEERER_MODELS}
					selectedModelId={steererModel}
					onSelectModel={(id) => (steererModel = id)}
					config={STEERER_CONFIG}
				/>

				<h2 class="sub">Sub-judges</h2>
				<WorkerCard
					name="refocus"
					label="Refocus"
					description="Injects a grounding message when the session drifts away from its anchor topic."
					enabled={refocusOn}
					onToggle={(v) => (refocusOn = v)}
					models={SUBJUDGE_MODELS}
					selectedModelId={refocusModel}
					onSelectModel={(id) => (refocusModel = id)}
					config={JUDGE_CONFIG}
				/>
				<WorkerCard
					name="verifier"
					label="Verifier"
					description="Nudges on the next turn when the assistant claims a task is done without supporting evidence."
					enabled={verifierOn}
					onToggle={(v) => (verifierOn = v)}
					models={SUBJUDGE_MODELS}
					selectedModelId={verifierModel}
					onSelectModel={(id) => (verifierModel = id)}
					config={JUDGE_CONFIG}
				/>
				<WorkerCard
					name="correction"
					label="Correction classifier"
					description="Classifies operator corrections so the steerer learns which interventions landed."
					enabled={correctionOn}
					onToggle={(v) => (correctionOn = v)}
					models={SUBJUDGE_MODELS}
					selectedModelId={correctionModel}
					onSelectModel={(id) => (correctionModel = id)}
					config={CORRECTION_CONFIG}
				/>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.shell {
		display: flex;
		height: 1180px;
		background: var(--surface-bg);
	}
	.nav {
		width: 240px;
		flex-shrink: 0;
		position: relative;
		display: flex;
		border-right: 1px solid var(--border-subtle);
	}
	.content {
		flex: 1 1 auto;
		min-width: 0;
		overflow-y: auto;
		padding: 28px 44px 64px;
		display: flex;
		flex-direction: column;
		gap: 18px;
	}
	.st-head {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.st-title {
		font-family: var(--font-display);
		font-size: 20px;
		font-weight: 700;
		color: var(--text-strong);
		margin: 0;
		letter-spacing: -0.01em;
	}
	.st-sub {
		margin: 0;
		font-size: 13px;
		color: var(--text-2);
		line-height: 1.6;
		max-width: 760px;
	}
	.sub {
		font-family: var(--font-display);
		font-size: 12px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 6px 0 -4px;
	}
	.env {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 14px;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--surface-2);
	}
	.env-status {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.env-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--text-3);
	}
	.env-dot--ok {
		background: var(--status-success-fg);
	}
	.env-label {
		font-size: 12px;
		color: var(--text-2);
	}
	.env-ok {
		color: var(--status-success-fg);
	}
	.env-row {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.env-strong {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-1);
	}
	.env-controls {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.env-input {
		flex: 1 1 240px;
		min-width: 0;
		padding: 6px 9px;
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-1);
		border-radius: 4px;
		font-size: 12px;
	}
	.env-btn {
		flex-shrink: 0;
		padding: 5px 12px;
		border: 1px solid var(--border-default);
		background: var(--surface-card);
		color: var(--text-1);
		border-radius: 4px;
		font-size: 11px;
		font-weight: 600;
		cursor: pointer;
	}
</style>
