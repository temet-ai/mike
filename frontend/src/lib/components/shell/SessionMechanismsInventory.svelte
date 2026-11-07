<script lang="ts">
	import { resolve } from '$app/paths';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	export type AuthorityCode = 'supply_context' | 'redirect' | 'command';
	export type ModeCode = 'augment' | 'drive';
	export type FilterRuleKind = 'text_substring' | 'lazy_section' | 'xml_tag';

	export interface SteererRow {
		steerer_id: number;
		display_name: string;
		name: string;
		description: string;
		authority: AuthorityCode;
		mode: ModeCode;
		model: string;
		enrolment_required: boolean;
		enabled: boolean;
		kind: string;
		enrolment: {
			enrolment_id: number;
			enrolled_at: string;
			source: 'default' | 'manual';
		} | null;
		activity: { fired: number; held: number };
	}

	export interface GraderRow {
		grader_name: string;
		display_name: string;
		enabled: boolean;
		this_session: { turns_graded: number; corrections: number; accepted: number } | null;
	}

	export interface FilterRuleRow {
		rule_id: number;
		kind: FilterRuleKind;
		label: string;
		tag: string | null;
		enabled: boolean;
		fired_turns: number;
	}

	export interface AnalyzerRow {
		analyzer_name: string;
		display_name: string;
		enabled: boolean;
		hit_count: number;
		is_intercept: boolean;
	}

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
				return 'color:var(--status-warning-fg);background:var(--status-warning-bg);--aug-border-bg:var(--status-warning-border);';
			case 'redirect':
				return 'color:var(--status-steering-fg);background:var(--status-steering-bg);--aug-border-bg:var(--status-steering-border);';
			case 'supply_context':
				return 'color:var(--text-1);background:var(--surface-2);--aug-border-bg:var(--border-subtle);';
		}
	}

	interface Props {
		steerers: SteererRow[];
		graders: GraderRow[];
		filterRules: FilterRuleRow[];
		analyzers: AnalyzerRow[];
		sessionActive: boolean;
		onEnrol: (steererId: number) => void;
		onDisenrol: (steererId: number) => void;
		onToggleRule?: (ruleId: number, enabled: boolean) => void;
		onToggleAnalyzer?: (analyzerName: string, enabled: boolean) => void;
		onToggleSteerer?: (steererId: number, enabled: boolean) => void;
		verifierCounts?: { fired: number; delivered: number } | null;
		topOpen?: boolean;
	}

	let {
		steerers: steerersProp,
		graders: gradersProp,
		filterRules: filterRulesProp,
		analyzers: analyzersProp,
		sessionActive,
		onEnrol,
		onDisenrol,
		onToggleSteerer,
		verifierCounts = null,
		topOpen: topOpenProp = false
	}: Props = $props();

	function asArray<T>(v: T[] | null | undefined): T[] {
		return Array.isArray(v) ? v : [];
	}
	const steerers = $derived(asArray(steerersProp));
	const graders = $derived(asArray(gradersProp));
	const filterRules = $derived(asArray(filterRulesProp));
	const analyzers = $derived(asArray(analyzersProp));

	// svelte-ignore state_referenced_locally
	let topOpen = $state(topOpenProp);

	let openSections = $state<Set<string>>(new Set(['steerers']));

	// eslint-disable-next-line svelte/prefer-writable-derived -- cannot be a plain $derived: handleEnrol/handleDisenrol below reassign `fleet` independently of `steerers` for optimistic UI updates that a derived value can't locally override
	let fleet = $state<SteererRow[]>([]);

	$effect(() => {
		fleet = steerers.map((r) => ({ ...r }));
	});

	let enrollingId = $state<number | null>(null);

	function toggleSection(id: string): void {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- copy-mutate-reassign pattern: `next` is a throwaway local, never read while being mutated; reactivity comes from replacing `openSections` below, not from in-place mutation
		const next = new Set(openSections);
		if (next.has(id)) {
			next.delete(id);
		} else {
			next.add(id);
		}
		openSections = next;
	}

	function handleEnrol(row: SteererRow): void {
		if (!sessionActive || enrollingId !== null) return;
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

	function handleDisenrol(row: SteererRow): void {
		if (!sessionActive || enrollingId !== null || !row.enrolment) return;
		enrollingId = row.steerer_id;
		fleet = fleet.map((r) => (r.steerer_id === row.steerer_id ? { ...r, enrolment: null } : r));
		onDisenrol(row.steerer_id);
		enrollingId = null;
	}

	function handleToggleSteerer(row: SteererRow): void {
		if (enrollingId !== null) return;
		const next = !row.enabled;
		fleet = fleet.map((r) => (r.steerer_id === row.steerer_id ? { ...r, enabled: next } : r));
		onToggleSteerer?.(row.steerer_id, next);
	}

	function formatTime(iso: string): string {
		try {
			return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		} catch {
			return iso;
		}
	}
</script>

<div class="smi-top-wrapper" class:smi-top-wrapper--open={topOpen}>
	<div class="smi-top-bar">
		<button
			type="button"
			class="smi-top-toggle"
			class:smi-top-toggle--open={topOpen}
			aria-expanded={topOpen}
			aria-controls="smi-panel-body"
			onclick={() => {
				topOpen = !topOpen;
			}}
		>
			<span class="smi-top-label">Session controls</span>
			{#if !topOpen}
				<span class="smi-top-summary">
					{fleet.filter((r) => !r.enrolment_required || r.enrolment !== null).length}/{fleet.length} steerers
					{#if fleet.some((r) => r.activity.fired > 0)}·
						{fleet.filter((r) => r.activity.fired > 0).length} fired{/if}
					· {graders.filter((g) => g.enabled).length} grader · {filterRules.filter((r) => r.enabled)
						.length} rules · {analyzers.filter((a) => a.enabled).length} analyzers
				</span>
			{/if}
		</button>
	</div>

	{#if topOpen}
		<div class="smi-panel" id="smi-panel-body">
			<div class="smi-panel-header">
				<h3 class="smi-hd">Session controls</h3>
				<p class="smi-panel-desc">
					Every <Tooltip slug="steerer">steerer</Tooltip>, <Tooltip slug="grader">grader</Tooltip>,
					and <Tooltip slug="analyzer">analyzer</Tooltip> that affects this session — enrolled or not,
					fired or not.
				</p>
			</div>

			<div class="smi-section" class:smi-section--open={openSections.has('steerers')}>
				<button
					type="button"
					class="smi-section-btn"
					class:smi-section-btn--open={openSections.has('steerers')}
					data-section="steerers"
					aria-expanded={openSections.has('steerers')}
					onclick={() => toggleSection('steerers')}
				>
					<DisclosureMarker open={openSections.has('steerers')} size={11} />
					<span class="smi-section-label">Steerers</span>
					<span class="smi-badge-backing">
						<span class="smi-badge" data-augmented-ui="tl-clip br-clip border">
							{fleet.filter((r) => !r.enrolment_required || r.enrolment !== null)
								.length}/{fleet.length} active ·
							{fleet.filter((r) => r.activity.fired > 0).length} fired
						</span>
					</span>
					{#if !sessionActive}
						<span class="smi-inactive-chip-backing">
							<span class="smi-inactive-chip" data-augmented-ui="tl-clip br-clip border"
								>read-only</span
							>
						</span>
					{/if}
				</button>

				<div
					class="smi-section-body"
					data-section="steerers"
					aria-hidden={!openSections.has('steerers')}
				>
					{#each fleet as row (row.steerer_id)}
						{@const enrolled = row.enrolment !== null}
						{@const isDefault = row.enrolment?.source === 'default'}
						{@const isAuto = !row.enrolment_required}
						{@const isVerifier = row.kind === 'verifier'}

						<div
							class="smi-row"
							class:smi-row--auto={isAuto && !isVerifier}
							class:smi-row--verifier={isVerifier}
							class:smi-row--enrolled={enrolled && !isAuto && !isVerifier}
							class:smi-row--not-enrolled={!enrolled && !isAuto && !isVerifier}
						>
							<div
								class="smi-rail"
								style="background:{isVerifier
									? row.enabled
										? 'var(--status-warning-fg)'
										: 'var(--border-subtle)'
									: isAuto || enrolled
										? 'var(--status-steering-fg)'
										: 'var(--border-subtle)'};"
								aria-hidden="true"
							></div>

							<div class="smi-td smi-td--name">
								<div class="smi-name-row">
									<span class="smi-name">{row.display_name}</span>
									<span class="smi-slug">{row.name}</span>
									{#if isVerifier}
										<span class="smi-fd-badge-backing">
											<span
												class="smi-fd-badge"
												data-augmented-ui="tl-clip br-clip border"
												title="verifier — always-on judge steerer; fires globally, not per-session enrolment."
												aria-label="Always-on judge steerer"
											>
												judge
											</span>
										</span>
									{:else if isAuto}
										<span class="smi-auto-badge-backing">
											<span
												class="smi-auto-badge"
												data-augmented-ui="tl-clip br-clip border"
												title="This steerer runs automatically — no per-session enrolment needed. DATA GAP: enrolment_required field pending on SteererDto."
												aria-label="Auto — runs without enrolment"
											>
												auto
											</span>
										</span>
									{/if}
								</div>
								<p class="smi-desc">{row.description}</p>
							</div>

							<div class="smi-td smi-td--abilities">
								<span class="smi-chip-backing">
									<abbr
										class="smi-chip"
										data-augmented-ui="tl-clip br-clip border"
										style={authorityStyle(row.authority)}
										title={AUTHORITY_TOOLTIPS[row.authority]}
									>
										{AUTHORITY_LABELS[row.authority]}
									</abbr>
								</span>
								<span class="smi-chip-backing">
									<abbr
										class="smi-chip"
										data-augmented-ui="tl-clip br-clip border"
										style="color:var(--text-1);background:var(--surface-2);--aug-border-bg:var(--border-subtle);"
										title={MODE_TOOLTIPS[row.mode]}
									>
										{MODE_LABELS[row.mode]}
									</abbr>
								</span>
							</div>

							<div class="smi-td smi-td--activity">
								{#if isVerifier}
									{#if verifierCounts !== null && verifierCounts !== undefined}
										<span
											class="smi-fd-counts"
											aria-label="fired {verifierCounts.fired}, delivered {verifierCounts.delivered}"
										>
											<span
												class="smi-fd-count"
												class:smi-fd-count--nonzero={verifierCounts.fired > 0}
											>
												{verifierCounts.fired} fired
											</span>
											<span class="smi-fd-delivered">
												{verifierCounts.delivered} delivered
											</span>
										</span>
									{:else}
										<span class="smi-zero-activity" aria-label="No verifier activity this session"
											>—</span
										>
									{/if}
								{:else if isAuto || row.activity.fired > 0 || row.activity.held > 0}
									<span
										class="smi-activity"
										aria-label="{row.activity.fired} fired, {row.activity.held} held"
									>
										<span
											class="smi-fired-count"
											class:smi-fired-count--nonzero={row.activity.fired > 0}
										>
											{row.activity.fired} fired
										</span>
										<span class="smi-held-count">
											{row.activity.held} held
										</span>
									</span>
								{:else}
									<span class="smi-zero-activity" aria-label="No activity this session">—</span>
								{/if}
							</div>

							<div class="smi-td smi-td--enrol">
								{#if isVerifier}
									<button
										type="button"
										role="switch"
										class="smi-global-toggle"
										class:smi-global-toggle--on={row.enabled}
										aria-checked={row.enabled}
										aria-label="{row.enabled ? 'Disable' : 'Enable'} {row.display_name} globally"
										title="{row.enabled
											? 'Click to disable'
											: 'Click to enable'} {row.display_name} fleet-wide"
										onclick={() => handleToggleSteerer(row)}
									>
										{row.enabled ? 'ON' : 'OFF'}
									</button>
									<span class="smi-global-label">global</span>
								{:else if isAuto}
									<span
										class="smi-enrol-status smi-enrol-status--auto"
										aria-label="Runs automatically"
									>
										runs automatically
									</span>
								{:else if enrolled}
									<div class="smi-enrol-info">
										<span class="smi-enrolled-chip-backing">
											<span class="smi-enrolled-chip" data-augmented-ui="tl-clip br-clip border">
												ENROLLED
											</span>
										</span>
										{#if isDefault}
											<span class="smi-source-backing">
												<span
													class="smi-source smi-source--default"
													data-augmented-ui="tl-clip br-clip border"
													title="Auto-enrolled by system configuration (Settings → Steerers). DATA GAP: source field pending on SessionEnrolmentRow."
												>
													system
												</span>
											</span>
										{:else}
											<span class="smi-source-backing">
												<span
													class="smi-source smi-source--manual"
													data-augmented-ui="tl-clip br-clip border"
													title="Manually enrolled for this session. DATA GAP: source field pending on SessionEnrolmentRow."
												>
													manual
												</span>
											</span>
										{/if}
										{#if row.enrolment?.enrolled_at}
											<span class="smi-enrol-time"
												>since {formatTime(row.enrolment.enrolled_at)}</span
											>
										{/if}
									</div>
									<button
										type="button"
										class="smi-disenrol-btn"
										disabled={!sessionActive || enrollingId === row.steerer_id}
										aria-label="Disenrol {row.display_name}"
										onclick={() => handleDisenrol(row)}
									>
										Disenrol
									</button>
								{:else}
									<span class="smi-not-enrolled" aria-label="Not enrolled">—</span>
									<button
										type="button"
										class="smi-enrol-btn"
										disabled={!sessionActive || enrollingId === row.steerer_id}
										aria-label="Enrol {row.display_name}"
										onclick={() => handleEnrol(row)}
									>
										Enrol
									</button>
								{/if}
							</div>
						</div>
					{/each}

					<div class="smi-section-footer">
						<span class="smi-section-add-backing">
							<a
								href={resolve('steering/new')}
								class="smi-section-add"
								data-augmented-ui="tr-clip bl-clip border"
								title="Create a new steerer rule"
							>
								+ Add steerer
							</a>
						</span>
					</div>
				</div>
			</div>

			<div class="smi-section" class:smi-section--open={openSections.has('graders')}>
				<button
					type="button"
					class="smi-section-btn"
					class:smi-section-btn--open={openSections.has('graders')}
					data-section="graders"
					aria-expanded={openSections.has('graders')}
					onclick={() => toggleSection('graders')}
				>
					<DisclosureMarker open={openSections.has('graders')} size={11} />
					<span class="smi-section-label">Graders</span>
					<span class="smi-badge-backing">
						<span class="smi-badge" data-augmented-ui="tl-clip br-clip border">
							{graders.filter((g) => g.enabled).length} active ·
							{graders.reduce((n, g) => n + (g.this_session?.corrections ?? 0), 0)} corrections
						</span>
					</span>
					<span class="smi-data-gap-note-backing">
						<span
							class="smi-data-gap-note"
							data-augmented-ui="tl-clip br-clip border"
							title="DATA GAP: per-session grader stats not yet exposed via API. Fixture data shown."
						>
							fixture data
						</span>
					</span>
				</button>

				<div
					class="smi-section-body"
					data-section="graders"
					aria-hidden={!openSections.has('graders')}
				>
					{#each graders as grader (grader.grader_name)}
						<div class="smi-grader-row">
							<div
								class="smi-rail"
								style="background:{grader.enabled
									? 'var(--status-success-fg)'
									: 'var(--border-subtle)'};"
								aria-hidden="true"
							></div>
							<div class="smi-td smi-td--name">
								<div class="smi-name-row">
									<span class="smi-name">{grader.display_name}</span>
									<span class="smi-slug">{grader.grader_name}</span>
									{#if grader.enabled}
										<span class="smi-on-chip-backing">
											<span
												class="smi-on-chip"
												data-augmented-ui="tl-clip br-clip border"
												aria-label="Enabled">ON</span
											>
										</span>
									{:else}
										<span class="smi-off-chip-backing">
											<span
												class="smi-off-chip"
												data-augmented-ui="tl-clip br-clip border"
												aria-label="Disabled">OFF</span
											>
										</span>
									{/if}
								</div>
							</div>
							<div class="smi-td smi-td--grader-stats">
								{#if grader.this_session}
									<span class="smi-stat">
										<strong>{grader.this_session.turns_graded}</strong> turns graded
									</span>
									<span class="smi-stat smi-stat--correction">
										<strong>{grader.this_session.corrections}</strong> corrections
									</span>
									<span class="smi-stat">
										<strong>{grader.this_session.accepted}</strong> accepted
									</span>
								{:else}
									<span class="smi-zero-activity">No grader activity this session</span>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			</div>

			<div class="smi-section" class:smi-section--open={openSections.has('rules')}>
				<button
					type="button"
					class="smi-section-btn"
					class:smi-section-btn--open={openSections.has('rules')}
					data-section="rules"
					aria-expanded={openSections.has('rules')}
					onclick={() => toggleSection('rules')}
				>
					<DisclosureMarker open={openSections.has('rules')} size={11} />
					<span class="smi-section-label">Filter Rules</span>
					<span class="smi-badge-backing">
						<span class="smi-badge" data-augmented-ui="tl-clip br-clip border">
							{filterRules.filter((r) => r.enabled).length} discipline rules ·
							{filterRules.reduce((n, r) => n + r.fired_turns, 0)} turns matched
						</span>
					</span>
					<span class="smi-data-gap-note-backing">
						<span
							class="smi-data-gap-note"
							data-augmented-ui="tl-clip br-clip border"
							title="DATA GAP: per-session filter rule firing attribution not yet implemented. Fixture data shown."
						>
							fixture data
						</span>
					</span>
				</button>

				<div class="smi-section-body" data-section="rules" aria-hidden={!openSections.has('rules')}>
					{#each filterRules as rule (rule.rule_id)}
						<div class="smi-rule-row">
							<div
								class="smi-rail"
								style="background:{rule.enabled && rule.fired_turns > 0
									? 'var(--interactive-active-fg)'
									: 'var(--border-subtle)'};"
								aria-hidden="true"
							></div>
							<div class="smi-td smi-td--name">
								<div class="smi-name-row">
									<span class="smi-name">Rule {rule.rule_id}</span>
									<span class="smi-rule-kind">{rule.kind}</span>
									{#if rule.tag}
										<span class="smi-rule-tag-backing">
											<span class="smi-rule-tag" data-augmented-ui="tl-clip br-clip border"
												>{rule.tag}</span
											>
										</span>
									{/if}
								</div>
								<p class="smi-desc">{rule.label}</p>
							</div>
							<div class="smi-td smi-td--rule-stats">
								{#if rule.fired_turns > 0}
									<span class="smi-stat smi-stat--active">
										fired <strong>{rule.fired_turns}</strong> turns this session
									</span>
								{:else}
									<span class="smi-zero-activity">not triggered this session</span>
								{/if}
							</div>
						</div>
					{/each}
					<div class="smi-rule-footer">
						<a href={resolve('/settings')} class="smi-view-all-link">View all rules →</a>
					</div>
				</div>
			</div>

			<div class="smi-section" class:smi-section--open={openSections.has('analyzers')}>
				<button
					type="button"
					class="smi-section-btn"
					class:smi-section-btn--open={openSections.has('analyzers')}
					data-section="analyzers"
					aria-expanded={openSections.has('analyzers')}
					onclick={() => toggleSection('analyzers')}
				>
					<DisclosureMarker open={openSections.has('analyzers')} size={11} />
					<span class="smi-section-label">Analyzers</span>
					<span class="smi-badge-backing">
						<span class="smi-badge" data-augmented-ui="tl-clip br-clip border">
							{analyzers.filter((a) => a.enabled).length} active ·
							{analyzers.reduce((n, a) => n + a.hit_count, 0)} hits
						</span>
					</span>
					<span class="smi-data-gap-note-backing">
						<span
							class="smi-data-gap-note"
							data-augmented-ui="tl-clip br-clip border"
							title="DATA GAP: per-session analyzer hit counts need a dedicated endpoint. Fixture data shown."
						>
							fixture data
						</span>
					</span>
				</button>

				<div
					class="smi-section-body"
					data-section="analyzers"
					aria-hidden={!openSections.has('analyzers')}
				>
					{#each analyzers as analyzer (analyzer.analyzer_name)}
						<div class="smi-analyzer-row" class:smi-analyzer-row--zero={analyzer.hit_count === 0}>
							<div
								class="smi-rail"
								style="background:{analyzer.enabled && analyzer.hit_count > 0
									? 'var(--status-warning-fg)'
									: 'var(--border-subtle)'};"
								aria-hidden="true"
							></div>
							<div class="smi-td smi-td--name">
								<div class="smi-name-row">
									<span class="smi-name" class:smi-name--inactive={!analyzer.enabled}
										>{analyzer.display_name}</span
									>
									{#if analyzer.is_intercept}
										<span class="smi-intercept-chip-backing">
											<span
												class="smi-intercept-chip"
												data-augmented-ui="tl-clip br-clip border"
												title="In-flight intercept — fires during the turn, not after"
												>intercept</span
											>
										</span>
									{/if}
									{#if !analyzer.enabled}
										<span class="smi-off-chip-backing">
											<span class="smi-off-chip" data-augmented-ui="tl-clip br-clip border"
												>OFF</span
											>
										</span>
									{/if}
								</div>
							</div>
							<div class="smi-td smi-td--analyzer-stats">
								{#if !analyzer.enabled}
									<span class="smi-zero-activity" aria-label="Disabled — no analysis run"
										>disabled</span
									>
								{:else if analyzer.hit_count > 0}
									<span class="smi-stat smi-stat--warning">
										<strong>{analyzer.hit_count}</strong>
										{analyzer.hit_count === 1 ? 'hit' : 'hits'} this session
									</span>
								{:else}
									<span class="smi-zero-activity" aria-label="Active, no hits this session">
										clean this session
									</span>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			</div>
		</div>
	{/if}
</div>

<style>
	.smi-top-wrapper {
		border-bottom: 1px solid var(--border-subtle);
	}

	.smi-top-bar {
		display: flex;
		align-items: center;
		background: var(--surface-2);
	}

	.smi-top-toggle {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 10px 20px;
		border: none;
		background: var(--surface-2);
		text-align: left;
		cursor: pointer;
		transition: background var(--dur-1) var(--ease-snap);
		flex-wrap: wrap;
	}

	.smi-top-toggle:hover {
		background: color-mix(in srgb, var(--surface-2) 70%, var(--border-subtle) 30%);
	}

	.smi-top-toggle--open,
	.smi-top-wrapper--open .smi-top-toggle {
		background: var(--surface-bg);
		box-shadow: inset 3px 0 0 0 var(--status-steering-fg);
	}

	.smi-top-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.smi-top-label {
		font-family: var(--font-display);
		font-size: 12px;
		font-weight: 800;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-2);
		flex-shrink: 0;
	}

	.smi-top-summary {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.smi-panel {
		background: var(--surface-bg);
	}

	.smi-panel-header {
		padding: 12px 20px;
		border-bottom: 1px solid var(--border-subtle);
	}

	.smi-hd {
		margin: 0 0 3px;
		font-family: var(--font-display);
		font-size: 11px;
		font-weight: 800;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-2);
	}

	.smi-panel-desc {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.smi-panel-desc :global(.tooltip-link) {
		color: inherit;
		text-decoration: underline dotted;
		text-decoration-color: var(--border-strong);
		text-underline-offset: 3px;
	}
	.smi-panel-desc :global(.tooltip-link:hover),
	.smi-panel-desc :global(.tooltip-link:focus-visible) {
		color: var(--text-strong);
		text-decoration-color: currentColor;
	}

	.smi-section {
		border-bottom: 1px solid var(--border-subtle);
	}

	.smi-section:last-child {
		border-bottom: none;
	}

	.smi-section-btn {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 10px 20px;
		border: none;
		background: var(--surface-2);
		text-align: left;
		cursor: pointer;
		transition: background var(--dur-1) var(--ease-snap);
		flex-wrap: wrap;
	}

	.smi-section-btn:hover {
		background: color-mix(in srgb, var(--surface-2) 60%, var(--border-subtle) 40%);
	}

	.smi-section-btn--open {
		background: var(--surface-bg);
		box-shadow: inset 3px 0 0 0 var(--status-steering-fg);
	}

	.smi-section-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.smi-section-label {
		font-family: var(--font-display);
		font-size: 12px;
		font-weight: 800;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-2);
		flex-shrink: 0;
	}

	.smi-badge-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-2);
	}
	.smi-badge {
		--edge: 3px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		background: var(--surface-2);
	}

	.smi-data-gap-note-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-2);
	}
	.smi-data-gap-note {
		--edge: 3px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-warning-border);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--status-warning-fg);
		background: var(--status-warning-bg);
		cursor: help;
	}

	.smi-inactive-chip-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-2);
	}
	.smi-inactive-chip {
		--edge: 3px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		background: var(--surface-2);
	}

	.smi-section-body {
		display: none;
	}

	.smi-section--open .smi-section-body {
		display: flex;
		flex-direction: column;
	}

	.smi-section-footer {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 12px 8px 28px;
		border-top: 1px solid var(--border-subtle);
	}

	.smi-section-add-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}

	.smi-section-add {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0px;
		--aug-br: 0px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		padding: 6px 14px;
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-1);
		text-decoration: none;
		transition: color var(--dur-1) var(--ease-snap);
	}

	.smi-section-add:hover {
		--aug-border-bg: var(--status-steering-border);
		color: var(--status-steering-fg);
	}

	.smi-section-add:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.smi-row,
	.smi-grader-row,
	.smi-rule-row,
	.smi-analyzer-row {
		display: flex;
		align-items: stretch;
		border-bottom: 1px solid var(--border-subtle);
		transition: background var(--dur-1) var(--ease-snap);
	}

	.smi-row:last-child,
	.smi-grader-row:last-child,
	.smi-rule-row:last-child {
		border-bottom: none;
	}

	.smi-analyzer-row:last-child {
		border-bottom: none;
	}

	.smi-row:hover,
	.smi-grader-row:hover,
	.smi-rule-row:hover,
	.smi-analyzer-row:hover {
		background: color-mix(in srgb, var(--surface-2) 35%, transparent 65%);
	}

	.smi-analyzer-row--zero {
		opacity: 0.7;
	}

	.smi-rail {
		flex: 0 0 3px;
		align-self: stretch;
	}

	.smi-td {
		padding: 10px 12px 10px 0;
		display: flex;
		flex-direction: column;
		justify-content: flex-start;
		min-width: 0;
	}

	.smi-td--name {
		flex: 0 1 300px;
		min-width: 140px;
		padding-left: 8px;
	}

	.smi-td--abilities {
		flex: 0 0 auto;
		flex-direction: row;
		flex-wrap: wrap;
		align-items: flex-start;
		gap: 4px;
		padding-top: 12px;
	}

	.smi-td--activity {
		flex: 0 0 auto;
		min-width: 70px;
		padding-top: 12px;
	}

	.smi-td--enrol {
		flex: 0 0 auto;
		min-width: 120px;
		padding-top: 10px;
		gap: 4px;
	}

	.smi-td--grader-stats,
	.smi-td--rule-stats,
	.smi-td--analyzer-stats {
		flex: 0 0 auto;
		min-width: 100px;
		padding-top: 12px;
		gap: 4px;
	}

	.smi-name-row {
		display: flex;
		align-items: baseline;
		gap: 6px;
		flex-wrap: wrap;
	}

	.smi-name {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
	}

	.smi-name--inactive {
		color: var(--text-3);
	}

	.smi-slug {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.smi-desc {
		margin: 4px 0 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.smi-chip-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}
	.smi-chip {
		--edge: 3px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-decoration: none;
		border-bottom: 1px dotted var(--border-default);
		cursor: help;
		flex-shrink: 0;
	}

	.smi-auto-badge-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}
	.smi-auto-badge {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		background: var(--status-steering-bg);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--status-steering-fg);
		cursor: help;
	}

	.smi-fd-badge-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}
	.smi-fd-badge {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-warning-border);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		background: var(--status-warning-bg);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--status-warning-fg);
		cursor: help;
	}

	.smi-row--verifier {
		background: color-mix(in srgb, var(--status-warning-bg) 20%, transparent 80%);
	}

	.smi-row--verifier:hover {
		background: color-mix(in srgb, var(--status-warning-bg) 35%, transparent 65%);
	}

	.smi-on-chip-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}
	.smi-on-chip {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-success-border);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		background: var(--status-success-bg);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		color: var(--status-success-fg);
	}

	.smi-off-chip-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}
	.smi-off-chip {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.smi-intercept-chip-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}
	.smi-intercept-chip {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-warning-border);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		background: var(--status-warning-bg);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--status-warning-fg);
		cursor: help;
	}

	.smi-rule-kind {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.smi-rule-tag-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}
	.smi-rule-tag {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-2);
	}

	.smi-activity {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.smi-fired-count {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.smi-fired-count--nonzero {
		color: var(--status-steering-fg);
		font-weight: 600;
	}

	.smi-held-count {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-faint);
	}

	.smi-fd-counts {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.smi-fd-count {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.smi-fd-count--nonzero {
		color: var(--status-warning-fg);
		font-weight: 600;
	}

	.smi-fd-delivered {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-faint);
	}

	.smi-zero-activity {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-faint);
	}

	.smi-stat {
		display: block;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
	}

	.smi-stat--correction {
		color: var(--status-steering-fg);
	}

	.smi-stat--active {
		color: var(--interactive-active-fg);
	}

	.smi-stat--warning {
		color: var(--status-warning-fg);
	}

	.smi-enrol-status--auto {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-steering-fg);
		font-style: italic;
	}

	.smi-global-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 38px;
		height: 22px;
		padding: 0 8px;
		border: 1px solid var(--border-default);
		border-radius: 2px;
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		color: var(--text-2);
		cursor: pointer;
		transition:
			background var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap),
			color var(--dur-1) var(--ease-snap);
	}

	.smi-global-toggle:hover {
		background: var(--surface-3);
		border-color: var(--border-strong);
	}

	.smi-global-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.smi-global-toggle--on {
		background: var(--status-success-bg);
		border-color: var(--status-success-fg);
		color: var(--status-success-fg);
	}

	.smi-global-toggle--on:hover {
		background: color-mix(in srgb, var(--status-success-bg) 80%, var(--surface-3) 20%);
	}

	.smi-global-label {
		display: block;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-faint);
		text-align: center;
		margin-top: 2px;
	}

	.smi-enrol-info {
		display: flex;
		flex-direction: column;
		gap: 3px;
		margin-bottom: 4px;
	}

	.smi-enrolled-chip-backing {
		display: inline-flex;
		flex-shrink: 0;
		align-self: flex-start;
		background: var(--surface-bg);
	}
	.smi-enrolled-chip {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border);
		display: inline-flex;
		align-items: center;
		width: fit-content;
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		background: var(--status-steering-bg);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--status-steering-fg);
	}

	.smi-source-backing {
		display: inline-flex;
		flex-shrink: 0;
		align-self: flex-start;
		background: var(--surface-bg);
	}
	.smi-source {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		display: inline-flex;
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 3px);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		width: fit-content;
		cursor: help;
		border-bottom: 1px dotted var(--border-default);
	}

	.smi-source--default {
		--aug-border-bg: var(--border-subtle);
		color: var(--text-3);
		background: var(--surface-2);
		border-bottom: 1px dotted var(--border-default);
	}

	.smi-source--manual {
		--aug-border-bg: var(--status-steering-border);
		color: var(--status-steering-fg);
		background: var(--status-steering-bg);
		border-bottom: 1px dotted var(--status-steering-fg);
	}

	.smi-enrol-time {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.smi-not-enrolled {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-faint);
		margin-bottom: 4px;
	}

	.smi-enrol-btn,
	.smi-disenrol-btn {
		display: inline-flex;
		align-items: center;
		padding: 3px 9px;
		border-radius: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 500;
		cursor: pointer;
		transition:
			background var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap),
			color var(--dur-1) var(--ease-snap);
		white-space: nowrap;
	}

	.smi-enrol-btn {
		border: 1px solid var(--status-steering-border);
		background: var(--status-steering-bg);
		color: var(--status-steering-fg);
	}

	.smi-enrol-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--status-steering-bg) 60%, var(--status-steering-fg) 40%);
	}

	.smi-disenrol-btn {
		border: 1px solid var(--border-default);
		background: var(--surface-2);
		color: var(--text-2);
	}

	.smi-disenrol-btn:hover:not(:disabled) {
		border-color: var(--status-danger-border);
		color: var(--status-danger-fg);
	}

	.smi-enrol-btn:disabled,
	.smi-disenrol-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.smi-enrol-btn:focus-visible,
	.smi-disenrol-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.smi-rule-footer {
		padding: 8px 20px;
		border-top: 1px solid var(--border-subtle);
	}

	.smi-view-all-link {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.smi-view-all-link:hover {
		color: var(--text-1);
	}

	.smi-view-all-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
