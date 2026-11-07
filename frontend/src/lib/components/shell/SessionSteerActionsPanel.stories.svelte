<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { resolve } from '$app/paths';
	import { userEvent, within, expect, fn } from 'storybook/test';

	type AuthorityCode = 'supply_context' | 'redirect' | 'command';
	type ModeCode = 'augment' | 'drive';

	const AUTHORITY_LABELS: Record<AuthorityCode, string> = {
		supply_context: 'Supply context',
		redirect: 'Redirect',
		command: 'Command'
	};

	const MODE_LABELS: Record<ModeCode, string> = {
		augment: 'Augment',
		drive: 'Drive'
	};

	const AUTHORITY_TOOLTIPS: Record<AuthorityCode, string> = {
		supply_context: 'supply_context — supply missing context/facts to the session.',
		redirect: 'redirect — call out the drift and point attention back to the thread.',
		command: 'command — issue a direct instruction.'
	};

	const MODE_TOOLTIPS: Record<ModeCode, string> = {
		augment: 'augment — adds a nudge alongside the conversation.',
		drive: 'drive — takes a stronger, driving steering role.'
	};

	function authorityStyle(a: AuthorityCode): string {
		switch (a) {
			case 'command':
				return 'color: var(--status-warning-fg); background: var(--status-warning-bg); border-color: var(--status-warning-border);';
			case 'redirect':
				return 'color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);';
			case 'supply_context':
				return 'color: var(--text-1); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	interface SessionFleetRow {
		steerer_id: number;
		display_name: string;
		name: string;
		description: string;
		authority: AuthorityCode;
		mode: ModeCode;
		model: string;
		enrolment: {
			enrolment_id: number;
			enrolled_at: string;
			source: 'default' | 'manual';
		} | null;
	}

	const INITIAL_FLEET: SessionFleetRow[] = [
		{
			steerer_id: 1,
			display_name: 'VerifierSteerer',
			name: 'verifier',
			description:
				'Watches for completion claims ("done", "fixed", "should work") when the evidence — ' +
				'test runs, committed files, build output — is absent from the same turn. ' +
				'Fires a redirect nudge on the next turn.',
			authority: 'redirect',
			mode: 'augment',
			model: 'claude-haiku-4-5-20251001',
			enrolment: {
				enrolment_id: 12,
				enrolled_at: '2026-06-26T08:00:00Z',
				source: 'default'
			}
		},
		{
			steerer_id: 2,
			display_name: 'ScopeGuard',
			name: 'scope_guard',
			description:
				'Detects when the assistant operates on files outside the declared allowlist or ' +
				'creates artefacts not in the plan. Fires a command injection to halt and surface ' +
				'the scope deviation.',
			authority: 'command',
			mode: 'drive',
			model: 'claude-haiku-4-5-20251001',
			enrolment: {
				enrolment_id: 13,
				enrolled_at: '2026-06-26T09:15:00Z',
				source: 'manual'
			}
		},
		{
			steerer_id: 3,
			display_name: 'ContextInjector',
			name: 'context_injector',
			description:
				'Monitors for turns where the assistant lacks key context about the codebase ' +
				'architecture. Injects the relevant context so the agent proceeds with accurate ' +
				'assumptions rather than inferring from partial information.',
			authority: 'supply_context',
			mode: 'augment',
			model: 'claude-haiku-4-5-20251001',
			enrolment: null
		}
	];

	function formatEnrolledAt(iso: string): string {
		try {
			return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		} catch {
			return iso;
		}
	}

	let sessionActive = $state(true);
	let enrollingId = $state<number | null>(null);
	let fleet = $state<SessionFleetRow[]>(INITIAL_FLEET.map((r) => ({ ...r })));

	const onEnrol = fn();
	const onDisenrol = fn();

	function handleEnrol(row: SessionFleetRow): void {
		if (enrollingId !== null || !sessionActive) return;
		enrollingId = row.steerer_id;
		fleet = fleet.map((r) =>
			r.steerer_id === row.steerer_id
				? {
						...r,
						enrolment: {
							enrolment_id: 999,
							enrolled_at: new Date().toISOString(),
							source: 'manual'
						}
					}
				: r
		);
		onEnrol(row.steerer_id);
		enrollingId = null;
	}

	function handleDisenrol(row: SessionFleetRow): void {
		if (enrollingId !== null || !sessionActive || !row.enrolment) return;
		enrollingId = row.steerer_id;
		fleet = fleet.map((r) => (r.steerer_id === row.steerer_id ? { ...r, enrolment: null } : r));
		onDisenrol(row.steerer_id);
		enrollingId = null;
	}

	const { Story } = defineMeta({
		title: 'Shell/SessionSteerActionsPanel',
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Per-session steer-actions area. Shows the steerer fleet roster with this ' +
						"session's enrolment state. Enrol / disenrol controls are gated on session " +
						'activity (deriveSessionActivity). Note / Summarize are future deferred ' +
						'placeholders. Default-on enrolments shown distinctly from manually-enrolled. ' +
						'DATA GAP: source field needed on SessionEnrolmentRow to distinguish default vs ' +
						'manual. See brief: artifacts/2026-06-26/session-hub-steer-design.md.'
				}
			}
		}
	});
</script>

<Story
	name="WithActiveSession"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvasElement.querySelector('.sap-hd')).not.toBeNull();
		await expect(canvasElement.querySelector('.sap-hd')?.textContent?.trim()).toContain(
			'Session Actions'
		);

		const steerTab = canvasElement.querySelector('.sap-tab--active');
		await expect(steerTab).not.toBeNull();
		await expect(steerTab?.textContent?.trim()).toContain('Steer');

		const futureTabs = canvasElement.querySelectorAll('.sap-tab--future');
		await expect(futureTabs.length).toBe(2);
		for (const tab of futureTabs) {
			await expect(tab.getAttribute('aria-disabled')).toBe('true');
		}

		const rows = canvasElement.querySelectorAll('.sap-row');
		await expect(rows.length).toBe(3);

		const defaultRow = canvasElement.querySelector('.sap-row--enrolled-default');
		await expect(defaultRow).not.toBeNull();
		await expect(defaultRow!.querySelector('.sap-source--default')?.textContent?.trim()).toBe(
			'system'
		);

		const manualRow = canvasElement.querySelector('.sap-row--enrolled-manual');
		await expect(manualRow).not.toBeNull();
		await expect(manualRow!.querySelector('.sap-source--manual')?.textContent?.trim()).toBe(
			'manual'
		);

		const notEnrolledRow = canvasElement.querySelector('.sap-row--not-enrolled');
		await expect(notEnrolledRow).not.toBeNull();
		const enrolBtn = notEnrolledRow!.querySelector('.sap-enrol-btn') as HTMLButtonElement | null;
		await expect(enrolBtn).not.toBeNull();
		await expect(enrolBtn!.disabled).toBe(false);

		const enrolledRows = canvasElement.querySelectorAll(
			'.sap-row--enrolled-default, .sap-row--enrolled-manual'
		);
		await expect(enrolledRows.length).toBe(2);
		for (const eRow of enrolledRows) {
			const dBtn = eRow.querySelector('.sap-disenrol-btn') as HTMLButtonElement | null;
			await expect(dBtn).not.toBeNull();
			await expect(dBtn!.disabled).toBe(false);
		}

		await userEvent.click(enrolBtn!);
		await expect(onEnrol).toHaveBeenCalledWith(3);

		const activeSwitch = canvas.getByRole('switch', { name: /session active/i });
		await userEvent.click(activeSwitch);

		const allActionBtns = canvasElement.querySelectorAll<HTMLButtonElement>(
			'.sap-enrol-btn, .sap-disenrol-btn'
		);
		for (const btn of allActionBtns) {
			await expect(btn.disabled).toBe(true);
		}

		await expect(canvasElement.querySelector('.sap-inactive-notice')).not.toBeNull();

		await expect(canvasElement.querySelector('.sap-add-link')).not.toBeNull();
	}}
>
	{#snippet template()}
		<div class="sap-harness">
			<span class="sap-harness__label">Harness:</span>
			<button
				type="button"
				role="switch"
				class="sap-harness__toggle"
				class:sap-harness__toggle--on={sessionActive}
				aria-label="Session active"
				aria-checked={sessionActive}
				onclick={() => {
					sessionActive = !sessionActive;
				}}
			>
				Session: {sessionActive ? 'ACTIVE' : 'INACTIVE'}
			</button>
			<span class="sap-harness__note"> ← toggle to verify inactive gate </span>
		</div>

		<div class="sap-panel">
			<div class="sap-header">
				<div class="sap-header__left">
					<span class="sap-zone-dot" aria-hidden="true"></span>
					<h3 class="sap-hd">Session Actions</h3>
				</div>

				<div class="sap-tabs" role="tablist" aria-label="Session action type">
					<button type="button" role="tab" class="sap-tab sap-tab--active" aria-selected="true">
						Steer
					</button>
					<button
						type="button"
						role="tab"
						class="sap-tab sap-tab--future"
						aria-selected="false"
						aria-disabled="true"
						tabindex="-1"
						title="Coming soon — take notes on this session"
					>
						Note
						<span class="sap-tab__badge" aria-hidden="true">soon</span>
					</button>
					<button
						type="button"
						role="tab"
						class="sap-tab sap-tab--future"
						aria-selected="false"
						aria-disabled="true"
						tabindex="-1"
						title="Coming soon — generate a session summary"
					>
						Summarize
						<span class="sap-tab__badge" aria-hidden="true">soon</span>
					</button>
				</div>

				<div class="sap-header__right">
					<a href={resolve('steering/new')} class="sap-add-link" title="Create a new steerer">
						+ New steerer
					</a>
				</div>
			</div>

			{#if !sessionActive}
				<div class="sap-inactive-notice" role="status" aria-live="polite">
					<span class="sap-inactive-notice__icon" aria-hidden="true">⏹</span>
					<span class="sap-inactive-notice__text">
						Session has closed — steer controls are read-only. The current enrolment configuration
						will carry over to the next active session.
					</span>
				</div>
			{/if}

			<div class="sap-fleet" aria-label="Steerer fleet for this session">
				<div class="sap-thead" aria-hidden="true">
					<span class="sap-th sap-th--name">Steerer</span>
					<span class="sap-th sap-th--abilities">Abilities</span>
					<span class="sap-th sap-th--status">Enrolment</span>
					<span class="sap-th sap-th--action">Action</span>
				</div>

				{#each fleet as row (row.steerer_id)}
					{@const enrolled = row.enrolment !== null}
					{@const isDefault = row.enrolment?.source === 'default'}
					<div
						class="sap-row"
						class:sap-row--enrolled-default={enrolled && isDefault}
						class:sap-row--enrolled-manual={enrolled && !isDefault}
						class:sap-row--not-enrolled={!enrolled}
					>
						<div
							class="sap-rail"
							style="background: {enrolled ? 'var(--status-steering-fg)' : 'var(--border-subtle)'};"
							aria-hidden="true"
						></div>

						<div class="sap-td sap-td--name">
							<div class="sap-name-group">
								<span class="sap-name">{row.display_name}</span><span class="sap-slug"
									>{row.name}</span
								>
							</div>
							<p class="sap-desc">{row.description}</p>
						</div>

						<div class="sap-td sap-td--abilities">
							<abbr
								class="sap-chip"
								style={authorityStyle(row.authority)}
								title={AUTHORITY_TOOLTIPS[row.authority]}
							>
								{AUTHORITY_LABELS[row.authority]}
							</abbr>
							<abbr
								class="sap-chip"
								style="color: var(--text-1); background: var(--surface-2); border-color: var(--border-subtle);"
								title={MODE_TOOLTIPS[row.mode]}
							>
								{MODE_LABELS[row.mode]}
							</abbr>
						</div>

						<div class="sap-td sap-td--status">
							{#if enrolled}
								<div class="sap-enrol-state">
									<span class="sap-enrolled-chip">
										<span class="sap-enrolled-dot" aria-hidden="true"></span>
										ENROLLED
									</span>
									{#if isDefault}
										<span
											class="sap-source sap-source--default"
											title="Auto-enrolled by system configuration (Settings → Steerers). DATA GAP: backend source field pending on SessionEnrolmentRow."
											aria-label="Enrolled by system default"
										>
											system
										</span>
									{:else}
										<span
											class="sap-source sap-source--manual"
											title="Manually enrolled for this session"
											aria-label="Manually enrolled"
										>
											manual
										</span>
									{/if}
									<span class="sap-enrol-time">
										since {formatEnrolledAt(row.enrolment!.enrolled_at)}
									</span>
								</div>
							{:else}
								<span class="sap-not-enrolled" aria-label="Not enrolled">—</span>
							{/if}
						</div>

						<div class="sap-td sap-td--action">
							{#if enrolled}
								<button
									type="button"
									class="sap-disenrol-btn"
									disabled={!sessionActive || enrollingId === row.steerer_id}
									aria-label="Disenrol {row.display_name} from this session"
									onclick={() => handleDisenrol(row)}
								>
									Disenrol
								</button>
							{:else}
								<button
									type="button"
									class="sap-enrol-btn"
									disabled={!sessionActive || enrollingId === row.steerer_id}
									aria-label="Enrol {row.display_name} in this session"
									onclick={() => handleEnrol(row)}
								>
									Enrol
								</button>
							{/if}
						</div>
					</div>
				{/each}
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.sap-harness {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 20px;
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
	}

	.sap-harness__label {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.sap-harness__toggle {
		padding: 3px 10px;
		border: 1px solid var(--border-default);
		border-radius: 3px;
		background: var(--surface-card);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		cursor: pointer;
		transition:
			background 0.12s,
			border-color 0.12s,
			color 0.12s;
	}

	.sap-harness__toggle--on {
		background: var(--status-success-bg);
		border-color: var(--status-success-border);
		color: var(--status-success-fg);
	}

	.sap-harness__toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.sap-harness__note {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.sap-panel {
		background: var(--surface-bg);
		border-top: 1px solid var(--border-subtle);
		border-bottom: 1px solid var(--border-subtle);
	}

	.sap-header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 20px 10px;
		border-bottom: 1px solid var(--border-subtle);
		flex-wrap: wrap;
	}

	.sap-header__left {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
	}

	.sap-header__right {
		margin-left: auto;
		flex-shrink: 0;
	}

	.sap-zone-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--status-steering-fg);
		flex-shrink: 0;
	}

	.sap-hd {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-2);
	}

	.sap-tabs {
		display: flex;
		align-items: center;
		gap: 2px;
	}

	.sap-tab {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 3px 10px;
		border: 1px solid transparent;
		border-radius: 4px;
		background: transparent;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-2);
		cursor: pointer;
		transition:
			color 0.12s,
			background 0.12s,
			border-color 0.12s;
	}

	.sap-tab--active {
		color: var(--text-strong);
		box-shadow: inset 0 -2px 0 0 var(--status-steering-fg);
	}

	.sap-tab--active:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.sap-tab--future {
		opacity: 0.45;
		cursor: not-allowed;
		pointer-events: none;
	}

	.sap-tab__badge {
		padding: 0 4px;
		border-radius: 3px;
		background: var(--surface-2);
		font-size: 9px;
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.sap-add-link {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 4px 10px;
		border: 1px solid var(--border-default);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-1);
		text-decoration: none;
		transition:
			border-color 0.12s,
			color 0.12s;
	}

	.sap-add-link:hover {
		border-color: var(--status-steering-border);
		color: var(--status-steering-fg);
	}

	.sap-add-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.sap-inactive-notice {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		padding: 10px 20px;
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
	}

	.sap-inactive-notice__icon {
		font-size: 12px;
		color: var(--text-3);
		flex-shrink: 0;
		line-height: 1.5;
	}

	.sap-inactive-notice__text {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.5;
		overflow-wrap: break-word;
	}

	.sap-fleet {
		display: flex;
		flex-direction: column;
	}

	.sap-thead {
		display: grid;
		grid-template-columns: 1fr 180px 160px 100px;
		gap: 0 12px;
		padding: 6px 20px 6px 23px;
		border-bottom: 1px solid var(--border-subtle);
	}

	.sap-th {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.sap-row {
		display: flex;
		align-items: stretch;
		border-bottom: 1px solid var(--border-subtle);
		transition: background 0.1s;
	}

	.sap-row:last-child {
		border-bottom: none;
	}

	.sap-row:hover {
		background: color-mix(in srgb, var(--surface-2) 35%, transparent 65%);
	}

	.sap-rail {
		flex: 0 0 3px;
		align-self: stretch;
	}

	.sap-td {
		padding: 10px 12px 10px 0;
		display: flex;
		flex-direction: column;
		justify-content: flex-start;
		min-width: 0;
	}

	.sap-td--name {
		flex: 1 1 0;
		padding-left: 8px;
	}

	.sap-td--abilities {
		flex: 0 0 180px;
		flex-direction: row;
		flex-wrap: wrap;
		align-items: flex-start;
		gap: 4px;
		padding-top: 12px;
	}

	.sap-td--status {
		flex: 0 0 160px;
		padding-top: 12px;
	}

	.sap-td--action {
		flex: 0 0 100px;
		justify-content: flex-start;
		padding-top: 10px;
	}

	.sap-name-group {
		display: flex;
		align-items: baseline;
		gap: 6px;
		flex-wrap: wrap;
	}

	.sap-name {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
		line-height: 1.3;
	}

	.sap-slug {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.sap-desc {
		margin: 4px 0 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sap-chip {
		padding: 1px 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-decoration: none;
		border-bottom: 1px dotted var(--border-default);
		cursor: help;
		flex-shrink: 0;
	}

	.sap-enrol-state {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.sap-enrolled-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--status-steering-fg);
	}

	.sap-enrolled-dot {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--status-steering-fg);
		flex-shrink: 0;
	}

	.sap-source {
		display: inline-flex;
		padding: 0 5px;
		border: 1px solid var(--border-subtle);
		border-radius: 3px;
		font-family: var(--font-mono, monospace);
		font-size: 9px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		cursor: help;
		border-bottom: 1px dotted var(--border-default);
		width: fit-content;
	}

	.sap-source--default {
		color: var(--text-3);
		background: var(--surface-2);
	}

	.sap-source--manual {
		color: var(--status-steering-fg);
		background: var(--status-steering-bg);
		border-color: var(--status-steering-border);
	}

	.sap-enrol-time {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.sap-not-enrolled {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-faint);
	}

	.sap-enrol-btn,
	.sap-disenrol-btn {
		display: inline-flex;
		align-items: center;
		padding: 4px 10px;
		border-radius: 4px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 500;
		cursor: pointer;
		transition:
			background 0.12s,
			border-color 0.12s,
			color 0.12s;
		white-space: nowrap;
	}

	.sap-enrol-btn {
		border: 1px solid var(--status-steering-border);
		background: var(--status-steering-bg);
		color: var(--status-steering-fg);
	}

	.sap-enrol-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--status-steering-bg) 70%, var(--status-steering-fg) 30%);
	}

	.sap-disenrol-btn {
		border: 1px solid var(--border-default);
		background: var(--surface-2);
		color: var(--text-2);
	}

	.sap-disenrol-btn:hover:not(:disabled) {
		border-color: var(--status-danger-border);
		color: var(--status-danger-fg);
	}

	.sap-enrol-btn:disabled,
	.sap-disenrol-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.sap-enrol-btn:focus-visible,
	.sap-disenrol-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
