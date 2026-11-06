<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import type { FilterRuleDto } from '$lib/types';

	function buildInjectReminder(
		overrides: Partial<{
			id: string;
			value: string;
			text: string | null;
			trigger_token: string | null;
			enabled: boolean;
		}> = {}
	): FilterRuleDto {
		return {
			id: overrides.id ?? 'inject-reminder-1',
			kind: 'inject_reminder' as FilterRuleDto['kind'],
			value: overrides.value ?? 'browser-guidance',
			action: 'append',
			text:
				overrides.text !== undefined
					? overrides.text
					: 'Use the browser_action tool to interact with web browsers. Always call screenshot() after each interaction to verify the result. Navigate with goto(), click elements with click(), and type with type().',
			enabled: overrides.enabled ?? true,
			source: 'ui',
			updated_at: '2026-06-19T10:00:00Z',
			sort_order: 1,
			trigger_token: overrides.trigger_token !== undefined ? overrides.trigger_token : '<chrome>'
		} as FilterRuleDto;
	}

	const RULE_INJECT_ENABLED = buildInjectReminder();

	const RULE_INJECT_DISABLED = buildInjectReminder({
		id: 'inject-reminder-disabled',
		enabled: false
	});

	const RULE_INJECT_NO_TRIGGER = buildInjectReminder({
		id: 'inject-reminder-no-trigger',
		trigger_token: null
	});

	const RULE_INJECT_LONG = buildInjectReminder({
		id: 'inject-reminder-long',
		value: 'comprehensive-browser-automation-guidance',
		text: `Use the browser_action tool to interact with web browsers. Always call screenshot() after each interaction to verify the result. Navigate with goto(), click elements with click(), type with type(), and scroll with scroll().

When automating browser tasks:
1. Start every session with screenshot() to confirm the initial state.
2. After every action (click, type, navigate) call screenshot() immediately.
3. If an expected element is not visible, scroll the page before retrying.
4. For forms: focus the field first, then type; do not assume autofocus.
5. Use waitForSelector() before interacting with dynamically-loaded elements.
6. Prefer aria-label selectors over XPath; CSS class selectors are fragile.
7. If a navigation causes a page reload, wait for domcontentloaded before interacting.

Never proceed past a failed action without logging the failure and taking a screenshot for diagnosis.`
	});

	const RULE_LAZY_CHROME = {
		id: 'lazy-chrome-compare',
		kind: 'lazy_section',
		value: '# Claude in Chrome browser automation',
		action: 'replace',
		text: '# Claude in Chrome browser automation\n[deferred — injected on trigger]',
		title: null,
		enabled: true,
		source: 'ui' as const,
		updated_at: '2026-06-19T09:00:00Z',
		sort_order: 0,
		trigger_token: '!!CHROME!!',
		fires_24h: 0,
		fires_7d: 0,
		last_fired_at: null
	} satisfies FilterRuleDto;

	const { Story } = defineMeta({
		title: 'Filter/InjectReminderDesign',
		component: {} as unknown as typeof import('./FilterRuleForm.svelte').default,
		tags: ['design-exploration']
	});
</script>

<Story name="CreateForm_Empty">
	{#snippet template()}
		<div class="w-[480px] p-4 bg-[var(--surface-bg)]">
			<form
				class="flex flex-col gap-3 p-3 rounded-md bg-[var(--surface-2)] border border-[var(--border-subtle)]"
				data-slot="inject-reminder-form-design"
				onsubmit={(e) => e.preventDefault()}
			>
				<div class="flex items-center justify-between gap-2">
					<h3 class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]">
						Add inject_reminder rule
					</h3>
					<span
						class="ir-kind-pill"
						title="inject_reminder — inject a reminder block into the next request when the model emits the trigger marker"
					>
						INJECT
					</span>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-label-id"
					>
						Label
					</span>
					<input
						type="text"
						autocomplete="off"
						spellcheck="false"
						placeholder="e.g. browser-guidance"
						aria-labelledby="ir-label-id"
						aria-describedby="ir-label-help"
						class="ir-text-input"
					/>
					<p id="ir-label-help" class="text-caption text-[var(--text-3)]">
						A short name for this reminder — used in the workbench rule card. Not matched against
						anything.
					</p>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-text-id"
					>
						Reminder text
					</span>
					<textarea
						rows="5"
						placeholder="The full reminder content to inject — e.g. Use browser_action for web interaction…"
						autocomplete="off"
						spellcheck="false"
						aria-labelledby="ir-text-id"
						aria-describedby="ir-text-help"
						class="ir-textarea"></textarea>
					<p id="ir-text-help" class="text-caption text-[var(--text-3)]">
						Content injected into the system field once per context window when the trigger fires.
						Write it as a complete, self-contained reminder — the model will see it verbatim.
					</p>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-trigger-id"
					>
						Trigger marker
					</span>
					<input
						type="text"
						autocomplete="off"
						spellcheck="false"
						placeholder="e.g. &lt;chrome&gt;"
						aria-labelledby="ir-trigger-id"
						aria-describedby="ir-trigger-help"
						class="ir-text-input"
					/>
					<p id="ir-trigger-help" class="text-caption text-[var(--text-3)]">
						Required. When the model emits this token in an assistant turn, the proxy injects the
						reminder into the next request. Pick a distinctive marker the model can emit
						intentionally (e.g. <code class="ir-inline-code">&lt;chrome&gt;</code>).
					</p>
				</div>

				<div class="flex items-center justify-end gap-2">
					<button type="button" class="ir-btn-cancel">Cancel</button>
					<button type="submit" disabled class="ir-btn-submit" aria-disabled="true">
						Add rule
					</button>
				</div>
			</form>
		</div>
	{/snippet}
</Story>

<Story name="CreateForm_FilledIn">
	{#snippet template()}
		<div class="w-[480px] p-4 bg-[var(--surface-bg)]">
			<form
				class="flex flex-col gap-3 p-3 rounded-md bg-[var(--surface-2)] border border-[var(--border-subtle)]"
				data-slot="inject-reminder-form-design"
				onsubmit={(e) => e.preventDefault()}
			>
				<div class="flex items-center justify-between gap-2">
					<h3 class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]">
						Add inject_reminder rule
					</h3>
					<span class="ir-kind-pill">INJECT</span>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-label-id-2"
					>
						Label
					</span>
					<input
						type="text"
						value="browser-guidance"
						autocomplete="off"
						spellcheck="false"
						aria-labelledby="ir-label-id-2"
						class="ir-text-input"
					/>
					<p class="text-caption text-[var(--text-3)]">
						A short name for this reminder — used in the workbench rule card. Not matched against
						anything.
					</p>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-text-id-2"
					>
						Reminder text
					</span>
					<textarea
						rows="5"
						autocomplete="off"
						spellcheck="false"
						aria-labelledby="ir-text-id-2"
						class="ir-textarea"
						>Use the browser_action tool to interact with web browsers. Always call screenshot() after each interaction to verify the result. Navigate with goto(), click elements with click(), and type with type().</textarea
					>
					<p class="text-caption text-[var(--text-3)]">
						Content injected into the system field once per context window when the trigger fires.
					</p>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-trigger-id-2"
					>
						Trigger marker
					</span>
					<input
						type="text"
						value="<chrome>"
						autocomplete="off"
						spellcheck="false"
						aria-labelledby="ir-trigger-id-2"
						class="ir-text-input"
					/>
					<p class="text-caption text-[var(--text-3)]">
						Required. When the model emits this token in an assistant turn, the proxy injects the
						reminder into the next request.
					</p>
				</div>

				<div class="flex items-center justify-end gap-2">
					<button type="button" class="ir-btn-cancel">Cancel</button>
					<button type="submit" class="ir-btn-submit">Add rule</button>
				</div>
			</form>
		</div>
	{/snippet}
</Story>

<Story name="CreateForm_ValidationState">
	{#snippet template()}
		<div class="w-[480px] p-4 bg-[var(--surface-bg)]">
			<form
				class="flex flex-col gap-3 p-3 rounded-md bg-[var(--surface-2)] border border-[var(--border-subtle)]"
				data-slot="inject-reminder-form-design"
				onsubmit={(e) => e.preventDefault()}
			>
				<div class="flex items-center justify-between gap-2">
					<h3 class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]">
						Add inject_reminder rule
					</h3>
					<span class="ir-kind-pill">INJECT</span>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-label-id-3"
					>
						Label
					</span>
					<input
						type="text"
						value="browser-guidance"
						autocomplete="off"
						spellcheck="false"
						aria-labelledby="ir-label-id-3"
						class="ir-text-input"
					/>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-text-id-3"
					>
						Reminder text
					</span>
					<textarea
						rows="5"
						autocomplete="off"
						spellcheck="false"
						aria-labelledby="ir-text-id-3"
						class="ir-textarea">Use the browser_action tool to interact with web browsers.</textarea
					>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-trigger-id-3"
					>
						Trigger marker
					</span>
					<input
						type="text"
						value=""
						autocomplete="off"
						spellcheck="false"
						placeholder="e.g. &lt;chrome&gt;"
						aria-labelledby="ir-trigger-id-3"
						aria-describedby="ir-trigger-help-3"
						class="ir-text-input ir-text-input--error"
					/>
					<p id="ir-trigger-help-3" class="text-caption text-[var(--status-danger-fg)]">
						Trigger marker is required. The model emits this token to request the reminder.
					</p>
				</div>

				<div class="flex items-center justify-end gap-2">
					<button type="button" class="ir-btn-cancel">Cancel</button>
					<button type="submit" disabled class="ir-btn-submit" aria-disabled="true">
						Add rule
					</button>
				</div>
			</form>
		</div>
	{/snippet}
</Story>

<Story name="EditForm_Populated">
	{#snippet template()}
		<div class="w-[480px] p-4 bg-[var(--surface-bg)]">
			<form
				class="flex flex-col gap-3 p-3 rounded-md bg-[var(--surface-2)] border border-[var(--border-subtle)]"
				data-slot="inject-reminder-form-design"
				onsubmit={(e) => e.preventDefault()}
			>
				<div class="flex items-center justify-between gap-2">
					<h3 class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]">
						Edit inject_reminder rule
					</h3>
					<span class="ir-kind-pill">INJECT</span>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-label-id-e"
					>
						Label
					</span>
					<input
						type="text"
						value="browser-guidance"
						autocomplete="off"
						spellcheck="false"
						aria-labelledby="ir-label-id-e"
						class="ir-text-input"
					/>
					<p class="text-caption text-[var(--text-3)]">
						A short name for this reminder — not matched against anything.
					</p>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-text-id-e"
					>
						Reminder text
					</span>
					<textarea
						rows="5"
						autocomplete="off"
						spellcheck="false"
						aria-labelledby="ir-text-id-e"
						class="ir-textarea"
						>Use the browser_action tool to interact with web browsers. Always call screenshot() after each interaction to verify the result. Navigate with goto(), click elements with click(), and type with type().</textarea
					>
					<p class="text-caption text-[var(--text-3)]">
						Content injected into the system field once per context window when the trigger fires.
					</p>
				</div>

				<div class="flex flex-col gap-1">
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)]"
						id="ir-trigger-id-e"
					>
						Trigger marker
					</span>
					<input
						type="text"
						value="<chrome>"
						autocomplete="off"
						spellcheck="false"
						aria-labelledby="ir-trigger-id-e"
						class="ir-text-input"
					/>
					<p class="text-caption text-[var(--text-3)]">
						When the model emits this token in an assistant turn, the proxy injects the reminder.
					</p>
				</div>

				<div class="flex items-center justify-end gap-2">
					<button type="button" class="ir-btn-cancel">Cancel</button>
					<button type="submit" class="ir-btn-submit">Save</button>
				</div>
			</form>
		</div>
	{/snippet}
</Story>

<Story name="RuleCard_Enabled">
	{#snippet template()}
		<div class="w-[480px] p-3 bg-[var(--surface-bg)]" role="list" aria-label="Workbench rule stack">
			<div
				class="ir-wb-card"
				data-slot="inject-reminder-card-design"
				data-enabled="true"
				role="listitem"
			>
				<div class="ir-wb-card__header">
					<button
						type="button"
						class="ir-drag-handle"
						aria-label="Reorder rule 1 — drag, or press arrow up / arrow down"
						title="Drag to reorder (or use arrow keys)"
					>
						<span class="ir-grip-icon" aria-hidden="true">⠿</span>
					</button>

					<span class="ir-rule-num" aria-label="Rule 1">1</span>

					<span
						class="ir-kind-pill"
						title="inject_reminder — inject a reminder block when the model emits the trigger marker"
					>
						INJECT
					</span>

					<span class="wb-trigger-chip" aria-label="Trigger marker &lt;chrome&gt;">
						&lt;chrome&gt;
					</span>

					<span class="ir-wb-spacer"></span>

					<label class="ir-toggle">
						<input
							type="checkbox"
							checked
							aria-label="Disable rule 1"
							style="accent-color: var(--interactive-active-fg); cursor: pointer;"
						/>
					</label>

					<button
						type="button"
						class="ir-icon-btn"
						aria-label="Edit this rule"
						title="Edit this rule"
					>
						✎
					</button>

					<button
						type="button"
						class="ir-icon-btn ir-icon-btn--danger"
						aria-label="Delete rule inject_reminder browser-guidance"
						title="Delete this rule"
					>
						⊗
					</button>
				</div>

				<div class="ir-wb-card__value">
					<code>browser-guidance</code>
				</div>

				<div class="ir-wb-card__reminder">
					<span class="ir-wb-card__reminder-label" aria-hidden="true">→</span>
					<code
						>Use the browser_action tool to interact with web browsers. Always call screenshot()
						after each interaction to verify the result. Navigate with goto(), click elements with
						click(), and type with type().</code
					>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="RuleCard_Disabled">
	{#snippet template()}
		<div class="w-[480px] p-3 bg-[var(--surface-bg)]" role="list" aria-label="Workbench rule stack">
			<div
				class="ir-wb-card ir-wb-card--disabled"
				data-slot="inject-reminder-card-design"
				data-enabled="false"
				role="listitem"
			>
				<div class="ir-wb-card__header">
					<button
						type="button"
						class="ir-drag-handle"
						aria-label="Reorder rule 1 — drag, or press arrow up / arrow down"
					>
						<span class="ir-grip-icon" aria-hidden="true">⠿</span>
					</button>
					<span class="ir-rule-num" aria-label="Rule 1">1</span>
					<span class="ir-kind-pill">INJECT</span>
					<span class="wb-trigger-chip" aria-label="Trigger marker &lt;chrome&gt;">
						&lt;chrome&gt;
					</span>
					<span class="ir-wb-spacer"></span>
					<label class="ir-toggle">
						<input
							type="checkbox"
							aria-label="Enable rule 1"
							style="accent-color: var(--interactive-active-fg); cursor: pointer;"
						/>
					</label>
					<button type="button" class="ir-icon-btn" aria-label="Edit this rule">✎</button>
					<button
						type="button"
						class="ir-icon-btn ir-icon-btn--danger"
						aria-label="Delete rule inject_reminder browser-guidance">⊗</button
					>
				</div>
				<div class="ir-wb-card__value">
					<code>browser-guidance</code>
				</div>
				<div class="ir-wb-card__reminder">
					<span class="ir-wb-card__reminder-label" aria-hidden="true">→</span>
					<code
						>Use the browser_action tool to interact with web browsers. Always call screenshot()
						after each interaction to verify the result.</code
					>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="RuleCard_NoTrigger">
	{#snippet template()}
		<div class="w-[480px] p-3 bg-[var(--surface-bg)]" role="list" aria-label="Workbench rule stack">
			<div
				class="ir-wb-card"
				data-slot="inject-reminder-card-design"
				data-enabled="true"
				role="listitem"
			>
				<div class="ir-wb-card__header">
					<button
						type="button"
						class="ir-drag-handle"
						aria-label="Reorder rule 1 — drag, or press arrow up / arrow down"
					>
						<span class="ir-grip-icon" aria-hidden="true">⠿</span>
					</button>
					<span class="ir-rule-num" aria-label="Rule 1">1</span>
					<span class="ir-kind-pill">INJECT</span>
					<span class="ir-wb-spacer"></span>
					<label class="ir-toggle">
						<input
							type="checkbox"
							checked
							aria-label="Disable rule 1"
							style="accent-color: var(--interactive-active-fg); cursor: pointer;"
						/>
					</label>
					<button type="button" class="ir-icon-btn" aria-label="Edit this rule">✎</button>
					<button
						type="button"
						class="ir-icon-btn ir-icon-btn--danger"
						aria-label="Delete rule inject_reminder browser-guidance">⊗</button
					>
				</div>
				<div class="ir-wb-card__value">
					<code>browser-guidance</code>
				</div>
				<div class="ir-wb-card__reminder">
					<span class="ir-wb-card__reminder-label" aria-hidden="true">→</span>
					<code>Use the browser_action tool to interact with web browsers.</code>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="RuleCard_LongReminder">
	{#snippet template()}
		<div class="w-[480px] p-3 bg-[var(--surface-bg)]" role="list" aria-label="Workbench rule stack">
			<div
				class="ir-wb-card"
				data-slot="inject-reminder-card-design"
				data-enabled="true"
				role="listitem"
			>
				<div class="ir-wb-card__header">
					<button
						type="button"
						class="ir-drag-handle"
						aria-label="Reorder rule 1 — drag, or press arrow up / arrow down"
					>
						<span class="ir-grip-icon" aria-hidden="true">⠿</span>
					</button>
					<span class="ir-rule-num" aria-label="Rule 1">1</span>
					<span class="ir-kind-pill">INJECT</span>
					<span class="wb-trigger-chip" aria-label="Trigger marker &lt;chrome&gt;">
						&lt;chrome&gt;
					</span>
					<span class="ir-wb-spacer"></span>
					<label class="ir-toggle">
						<input
							type="checkbox"
							checked
							aria-label="Disable rule 1"
							style="accent-color: var(--interactive-active-fg); cursor: pointer;"
						/>
					</label>
					<button type="button" class="ir-icon-btn" aria-label="Edit this rule">✎</button>
					<button
						type="button"
						class="ir-icon-btn ir-icon-btn--danger"
						aria-label="Delete rule inject_reminder comprehensive-browser-automation-guidance"
						>⊗</button
					>
				</div>
				<div class="ir-wb-card__value">
					<code>comprehensive-browser-automation-guidance</code>
				</div>
				<div class="ir-wb-card__reminder">
					<span class="ir-wb-card__reminder-label" aria-hidden="true">→</span>
					<code
						>Use the browser_action tool to interact with web browsers. Always call screenshot()
						after each interaction to verify the result. Navigate with goto(), click elements with
						click(), type with type(), and scroll with scroll(). When automating browser tasks: 1.
						Start every session with screenshot() to confirm the initial state. 2. After every
						action (click, type, navigate) call screenshot() immediately. 3. If an expected element
						is not visible, scroll the page before retrying. 4. For forms: focus the field first,
						then type; do not assume autofocus. 5. Use waitForSelector() before interacting with
						dynamically-loaded elements. 6. Prefer aria-label selectors over XPath; CSS class
						selectors are fragile. 7. If a navigation causes a page reload, wait for
						domcontentloaded before interacting. Never proceed past a failed action without logging
						the failure and taking a screenshot for diagnosis.</code
					>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="SideBySide_WithLazy">
	{#snippet template()}
		<div class="w-[520px] p-3 bg-[var(--surface-bg)]" role="list" aria-label="Workbench rule stack">
			<div
				class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] mb-2"
				aria-hidden="true"
			>
				inject_reminder (new kind — blue pill, no action badge)
			</div>

			<div
				class="ir-wb-card"
				data-slot="inject-reminder-card-design"
				data-enabled="true"
				role="listitem"
				style="margin-bottom: 8px;"
			>
				<div class="ir-wb-card__header">
					<button
						type="button"
						class="ir-drag-handle"
						aria-label="Reorder rule 1 — drag, or press arrow up / arrow down"
					>
						<span class="ir-grip-icon" aria-hidden="true">⠿</span>
					</button>
					<span class="ir-rule-num" aria-label="Rule 1">1</span>
					<span class="ir-kind-pill" title="inject_reminder">INJECT</span>
					<span class="wb-trigger-chip" aria-label="Trigger marker &lt;chrome&gt;">
						&lt;chrome&gt;
					</span>
					<span class="ir-wb-spacer"></span>
					<label class="ir-toggle">
						<input
							type="checkbox"
							checked
							aria-label="Disable rule 1"
							style="accent-color: var(--interactive-active-fg); cursor: pointer;"
						/>
					</label>
					<button type="button" class="ir-icon-btn" aria-label="Edit this rule">✎</button>
					<button
						type="button"
						class="ir-icon-btn ir-icon-btn--danger"
						aria-label="Delete rule inject_reminder browser-guidance">⊗</button
					>
				</div>
				<div class="ir-wb-card__value">
					<code>browser-guidance</code>
				</div>
				<div class="ir-wb-card__reminder">
					<span class="ir-wb-card__reminder-label" aria-hidden="true">→</span>
					<code
						>Use the browser_action tool to interact with web browsers. Always call screenshot()
						after each interaction.</code
					>
				</div>
			</div>

			<div
				class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] mb-2 mt-2"
				aria-hidden="true"
			>
				lazy_section (existing kind — amber pill + REPLACE badge)
			</div>

			<div class="wb-rule-card" role="listitem">
				<div class="wb-rule-card__header">
					<button
						type="button"
						class="wb-rule-drag-handle"
						aria-label="Reorder rule 2 — drag, or press arrow up / arrow down"
						title="Drag to reorder (or use arrow keys)"
					>
						<span style="font-size: 12px;" aria-hidden="true">⠿</span>
					</button>
					<span class="wb-rule-num" aria-label="Rule 2">2</span>
					<span class="wb-kind-pill" data-kind="lazy_section" title="lazy_section">LAZY</span>
					<span class="wb-action-badge" data-action="replace">replace</span>
					<span class="wb-trigger-chip" aria-label="Trigger token !!CHROME!!">!!CHROME!!</span>
					<span class="wb-rule-card__spacer"></span>
					<label class="wb-rule-toggle">
						<input
							type="checkbox"
							checked
							aria-label="Disable rule 2"
							style="accent-color: var(--interactive-active-fg); cursor: pointer;"
						/>
					</label>
					<button
						type="button"
						class="ir-icon-btn"
						aria-label="Edit this rule"
						title="Edit this rule">✎</button
					>
					<button
						type="button"
						class="ir-icon-btn ir-icon-btn--danger"
						aria-label="Delete rule lazy_section # Claude in Chrome browser automation">⊗</button
					>
				</div>
				<div class="wb-rule-card__value">
					<code># Claude in Chrome browser automation</code>
				</div>
				<div class="wb-rule-card__replacement">
					<span class="wb-rule-card__replacement-label" aria-hidden="true">→</span>
					<code># Claude in Chrome browser automation&#10;[deferred — injected on trigger]</code>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.ir-kind-pill {
		display: inline-flex;
		align-items: center;
		padding: 1px 5px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		letter-spacing: 0.05em;
		border-radius: 3px;
		border: 1px solid var(--status-info-border);
		background: var(--status-info-bg);
		color: var(--status-info-fg);
	}

	.ir-wb-card {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 5px;
		background: var(--bg);
		transition:
			opacity 120ms ease,
			border-color 120ms ease,
			box-shadow 120ms ease;
	}

	.ir-wb-card--disabled {
		opacity: 0.55;
	}

	.ir-wb-card__header {
		display: flex;
		align-items: center;
		gap: 5px;
		flex-wrap: wrap;
	}

	.ir-wb-spacer {
		flex: 1;
	}

	.ir-wb-card__value {
		padding: 3px 6px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 3px;
	}
	.ir-wb-card__value code {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		white-space: pre-wrap;
		word-break: break-word;
	}

	.ir-wb-card__reminder {
		display: flex;
		align-items: flex-start;
		gap: 4px;
		padding: 3px 6px;
		background: var(--status-warning-bg);
		border: 1px solid var(--status-warning-border);
		border-radius: 3px;
	}
	.ir-wb-card__reminder-label {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--status-warning-fg);
		flex-shrink: 0;
	}
	.ir-wb-card__reminder code {
		font: italic 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--status-warning-fg);
		white-space: pre-wrap;
		word-break: break-word;
	}

	.ir-drag-handle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 2px;
		color: var(--text-3);
		background: transparent;
		border: none;
		border-radius: 3px;
		cursor: grab;
	}
	.ir-drag-handle:hover {
		color: var(--text-strong);
		background: var(--surface-2);
	}
	.ir-drag-handle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring, var(--interactive-focus-ring));
	}

	.ir-rule-num {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-3);
		border: 1px solid var(--border-subtle);
		border-radius: 3px;
		flex-shrink: 0;
	}

	.ir-toggle {
		display: inline-flex;
		align-items: center;
	}

	.ir-icon-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		font-size: 13px;
		color: var(--text-2);
		background: transparent;
		border: none;
		border-radius: 4px;
		cursor: pointer;
	}
	.ir-icon-btn:hover {
		background: var(--surface-2);
		color: var(--text-strong);
	}
	.ir-icon-btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.ir-icon-btn--danger {
		color: var(--status-danger-fg);
	}
	.ir-icon-btn--danger:hover {
		color: var(--status-danger-fg);
		background: var(--status-danger-bg);
	}

	.ir-grip-icon {
		font-size: 12px;
		display: inline-flex;
	}

	.ir-text-input {
		width: 100%;
		border-radius: 4px;
		border: 1px solid var(--border-color);
		background: var(--bg);
		padding: 6px 8px;
		font: 12px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		outline: none;
		box-sizing: border-box;
	}
	.ir-text-input:focus-visible {
		border-color: var(--interactive-focus-ring);
		box-shadow: 0 0 0 1px var(--interactive-focus-ring);
	}
	.ir-text-input--error {
		border-color: var(--status-danger-fg);
	}
	.ir-text-input--error:focus-visible {
		box-shadow: 0 0 0 1px var(--status-danger-fg);
	}

	.ir-textarea {
		width: 100%;
		min-height: 7rem;
		max-height: 24rem;
		resize: vertical;
		border-radius: 4px;
		border: 1px solid var(--border-color);
		background: var(--bg);
		padding: 6px 8px;
		font: 12px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		outline: none;
		box-sizing: border-box;
	}
	.ir-textarea:focus-visible {
		border-color: var(--interactive-focus-ring);
		box-shadow: 0 0 0 1px var(--interactive-focus-ring);
	}

	.ir-btn-cancel {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		height: 28px;
		padding: 0 12px;
		font: 12px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-1);
		background: transparent;
		border: 1px solid var(--border-color);
		border-radius: 5px;
		cursor: pointer;
	}
	.ir-btn-cancel:hover {
		background: var(--surface-2);
	}
	.ir-btn-cancel:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.ir-btn-submit {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		height: 28px;
		padding: 0 12px;
		font: 12px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--surface-bg);
		background: var(--interactive-active-fg);
		border: 1px solid var(--interactive-active-fg);
		border-radius: 5px;
		cursor: pointer;
	}
	.ir-btn-submit:hover:not(:disabled) {
		opacity: 0.88;
	}
	.ir-btn-submit:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}
	.ir-btn-submit:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.ir-inline-code {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		background: var(--surface-2);
		padding: 0 3px;
		border-radius: 3px;
	}

	.wb-rule-card {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 5px;
		background: var(--bg);
	}
	.wb-rule-card__header {
		display: flex;
		align-items: center;
		gap: 5px;
		flex-wrap: wrap;
	}
	.wb-rule-card__spacer {
		flex: 1;
	}
	.wb-rule-drag-handle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 2px;
		color: var(--text-3);
		background: transparent;
		border: none;
		border-radius: 3px;
		cursor: grab;
	}
	.wb-rule-num {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-3);
		border: 1px solid var(--border-subtle);
		border-radius: 3px;
		flex-shrink: 0;
	}
	.wb-kind-pill {
		display: inline-flex;
		align-items: center;
		padding: 1px 5px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		letter-spacing: 0.05em;
		border-radius: 3px;
		border: 1px solid var(--border-color);
		background: var(--surface-2);
		color: var(--text-2);
	}
	.wb-kind-pill[data-kind='lazy_section'] {
		color: var(--status-warning-fg);
	}
	.wb-trigger-chip {
		display: inline-flex;
		align-items: center;
		padding: 0 5px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		border-radius: 999px;
		background: var(--status-warning-bg);
		border: 1px solid var(--status-warning-border);
		color: var(--status-warning-fg);
		white-space: pre-wrap;
		word-break: break-word;
	}
	.wb-action-badge {
		display: inline-flex;
		align-items: center;
		padding: 1px 5px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		border-radius: 3px;
		border: 1px solid transparent;
	}
	.wb-action-badge[data-action='replace'] {
		color: var(--status-warning-fg);
		background: var(--status-warning-bg);
		border-color: var(--status-warning-border);
	}
	.wb-rule-toggle {
		display: inline-flex;
		align-items: center;
	}
	.wb-rule-card__value {
		padding: 3px 6px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 3px;
	}
	.wb-rule-card__value code {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		white-space: pre-wrap;
		word-break: break-word;
	}
	.wb-rule-card__replacement {
		display: flex;
		align-items: flex-start;
		gap: 4px;
		padding: 3px 6px;
		background: var(--status-warning-bg);
		border: 1px solid var(--status-warning-border);
		border-radius: 3px;
	}
	.wb-rule-card__replacement-label {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--status-warning-fg);
		flex-shrink: 0;
	}
	.wb-rule-card__replacement code {
		font: italic 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--status-warning-fg);
		white-space: pre-wrap;
		word-break: break-word;
	}
</style>
