<!--
Design mock for the Wright page (artifacts/2026-10-16/resident/PAGE_SPEC.md). The page does not
exist yet: this file renders the proposed markup with mock data drawn from DESIGN.md, so the
snippets below stand in for the components the spec asks frontend-developer to build
(WrightMessage, WrightDecision, WrightChange, WrightProblemRow, WrightActionRow, WrightFixRow,
LinkChipRow). When the route lands, this file becomes its page story and imports ./+page.svelte.
The five stories are the states the operator named: empty, normal, a decision waiting, a steer
just applied, narrow width.
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, expect, within, waitFor } from 'storybook/test';

	const { Story } = defineMeta({
		title: 'Routes/Wright',
		parameters: { layout: 'fullscreen' }
	});

	type Seg = string | { text: string; href: string; external?: boolean };
	type LinkKind = 'problem' | 'action' | 'undo' | 'measure' | 'fix' | 'evidence';
	interface MsgLink {
		kind: LinkKind;
		text: string;
		href?: string;
	}
	interface Change {
		actionId: string;
		icon: 'pause' | 'open' | 'off';
		title: string;
		detail: string;
	}
	interface Answer {
		id: string;
		label: string;
		look: 'default' | 'outline' | 'ghost';
		change?: Change;
		reply?: string;
	}
	interface Decision {
		id: string;
		question: string;
		evidence: Seg[][];
		links: MsgLink[];
		answers: Answer[];
	}
	interface Message {
		id: string;
		from: 'you' | 'wright';
		day: string;
		at: string;
		body: Seg[][];
		links?: MsgLink[];
		change?: Change;
		decision?: Decision;
	}
	type ProblemStatus = 'needs-you' | 'working' | 'watching' | 'fixed';
	interface Problem {
		id: string;
		title: string;
		opened: string;
		openedBy: 'you' | 'Wright';
		status: ProblemStatus;
		detail: Seg[];
		links: MsgLink[];
		decisionId?: string;
		afterAnswer?: Record<string, { status: ProblemStatus; detail: Seg[] }>;
	}
	interface Action {
		id: string;
		day: string;
		at: string;
		what: Seg[];
		undo: 'Undo' | 'Revert' | null;
		noUndo?: string;
		links: MsgLink[];
	}
	type Verdict = 'better' | 'no-change' | 'measuring';
	interface KnownFix {
		id: string;
		name: string;
		verdict: Verdict;
		measure: Seg[];
		since: string;
		pausedBy?: string;
	}
	interface Cost {
		spent: string;
		problems: number;
		cached: string;
	}
	interface Scenario {
		messages: Message[];
		problems: Problem[];
		actions: Action[];
		fixes: KnownFix[];
		cost: Cost | null;
	}

	const CR3_COORDINATOR = 'e74ba661-2f0c-4d1b-9a3e-5b7c1d0e8f21';
	const TOOLING_T11 = 'e74bb772-8c41-4e0a-b6d2-0f9a3c7e5d14';
	const RUST_DEV_2 = '5d20c9e1-7b3a-4f86-a1c4-92e6b0d3f7a8';
	const ENGINEER_FORKS = '9b1f4e70-33c2-4a5d-8e19-c6d0a7f2b544';
	const ENGINEER_PATHS = '2c8a0d51-6e94-4b1f-a7c3-d58e1f9b0263';
	const ENGINEER_LAUNCHES = 'a4e7c219-0b5d-4f38-9c61-7e2d8a5b1f90';
	const BOUNCED_THREAD = 'th-7f3a9c21-4b8e-4d10-a2f6-c95e0b7d1384';
	const REPO = 'https://github.com/temet-ai/mike/commit/';

	const session = (id: string, text: string): Seg => ({ text, href: `/sessions/${id}` });

	const NORMAL_MESSAGES: Message[] = [
		{
			id: 'm1',
			from: 'you',
			day: 'Yesterday',
			at: '18:52',
			body: [['Why did you bounce that mail to tooling-T11?']]
		},
		{
			id: 'm2',
			from: 'wright',
			day: 'Yesterday',
			at: '18:52',
			body: [
				[
					session(TOOLING_T11, 'tooling-T11'),
					' closed at 18:31, when its task in change-record-3 was done. At 18:36 ',
					session(CR3_COORDINATOR, 'coordinator-e74ba661'),
					' sent it a question, so I ',
					{ text: 'bounced it', href: `/comms?thread=${BOUNCED_THREAD}` },
					' straight away and told the coordinator that ',
					session(RUST_DEV_2, 'rust-developer-2'),
					' holds that task now.'
				],
				['Before this fix, mail like that waited six hours and then expired unread.']
			],
			links: [
				{ kind: 'problem', text: 'Mail sent to a closed seat', href: '#wright-action-a5' },
				{ kind: 'action', text: 'Bounced 1 mail', href: '#wright-action-a5' },
				{ kind: 'undo', text: 'None: mail cannot be unsent' },
				{ kind: 'measure', text: '0 of 41 left waiting, 9 of 12 without the fix', href: '#wright-fix-k1' }
			]
		},
		{
			id: 'm3',
			from: 'you',
			day: 'Today',
			at: '09:20',
			body: [["Did the fix for the missing 'from' errors work?"]]
		},
		{
			id: 'm4',
			from: 'wright',
			day: 'Today',
			at: '09:20',
			body: [
				[
					'Yes. In the two weeks before it landed, 65 calls to send_session_message failed because they left out who the mail was from. Since it landed yesterday at 16:40, none have, out of 412 calls. The ',
					{ text: 'tool errors page', href: '/tools?tool_name=send_session_message' },
					' shows both periods.'
				]
			],
			links: [
				{ kind: 'problem', text: 'Mail tools failing when the sender is left out', href: '#wright-action-a6' },
				{ kind: 'action', text: 'Landed 4c1e9a2 on main', href: '#wright-action-a6' },
				{ kind: 'undo', text: 'Revert 4c1e9a2', href: '#wright-action-a6' },
				{ kind: 'measure', text: '65 failures before, 0 since', href: '/tools?tool_name=send_session_message' }
			]
		},
		{
			id: 'm5',
			from: 'you',
			day: 'Today',
			at: '10:12',
			body: [['Look at why forks start cold.']]
		},
		{
			id: 'm6',
			from: 'wright',
			day: 'Today',
			at: '10:12',
			body: [
				[
					'Opened it. 31 of the last 40 forks read less than a fifth of their start from the cache, so most forks pay for a full start. ',
					session(ENGINEER_FORKS, 'The engineer'),
					' is on it, and I will write here when I know why.'
				]
			],
			change: {
				actionId: 'a-forks',
				icon: 'open',
				title: 'Opened: Forks start without a warm cache',
				detail: 'You asked at 10:12'
			},
			links: [{ kind: 'problem', text: 'Forks start without a warm cache', href: '#wright-problem-p1' }]
		}
	];

	const DECISION_MESSAGE: Message = {
		id: 'm7',
		from: 'wright',
		day: 'Today',
		at: '21:02',
		body: [],
		decision: {
			id: 'd-judge',
			question: 'Turn the judge off until you add OpenRouter credit?',
			evidence: [
				[
					'The OpenRouter credit ran out at 20:05. Since then 148 of 672 judge calls have failed (22%). The hour before, 2% did. Each failed call leaves a turn with no judgement.'
				],
				[
					'I cannot add credit, and I do not change the judges myself. Credit is added on the ',
					{ text: 'worker settings page', href: '/settings/workers' },
					'.'
				]
			],
			links: [
				{ kind: 'problem', text: 'Judge calls failing', href: '#wright-problem-p4' },
				{ kind: 'evidence', text: 'Worker settings', href: '/settings/workers' }
			],
			answers: [
				{
					id: 'off',
					label: 'Turn judge off',
					look: 'default',
					change: {
						actionId: 'a-judge-off',
						icon: 'off',
						title: 'Turned off: the judge',
						detail: 'Until you add credit · you answered at 21:04'
					}
				},
				{
					id: 'credit',
					label: 'Credit added',
					look: 'outline',
					reply: 'You said credit was added. I will check the next 50 judge calls and write here.'
				},
				{
					id: 'dismiss',
					label: 'Dismiss',
					look: 'ghost',
					reply: 'Dismissed. The problem stays open under Open now.'
				}
			]
		}
	};

	const STEER_MESSAGES: Message[] = [
		{
			id: 'm8',
			from: 'you',
			day: 'Today',
			at: '21:10',
			body: [['Stop touching fleet mail until the run ends.']]
		},
		{
			id: 'm9',
			from: 'wright',
			day: 'Today',
			at: '21:10',
			body: [
				[
					'Done. While ',
					{ text: 'change-record-4', href: '/fleet' },
					' runs I will not bounce its mail. Mail to its closed seats waits as it did before, and I will tell its coordinator what I see instead. This switches back on when the fleet closes.'
				]
			],
			change: {
				actionId: 'a-steer',
				icon: 'pause',
				title: 'Paused: bounce mail during fleet runs',
				detail: 'Until change-record-4 closes · you asked at 21:10'
			},
			links: [
				{ kind: 'action', text: 'Paused at your request', href: '#wright-action-a-steer' },
				{ kind: 'fix', text: 'Bounce mail sent to a closed seat', href: '#wright-fix-k1' }
			]
		}
	];

	const NORMAL_PROBLEMS: Problem[] = [
		{
			id: 'p1',
			title: 'Forks start without a warm cache',
			opened: 'Today 10:12',
			openedBy: 'you',
			status: 'working',
			detail: ['31 of the last 40 forks read less than a fifth of their start from the cache.'],
			links: [{ kind: 'evidence', text: "The engineer's session", href: `/sessions/${ENGINEER_FORKS}` }]
		},
		{
			id: 'p2',
			title: 'Subagents told they are in a worktree with Mac paths',
			opened: 'Yesterday 07:00',
			openedBy: 'Wright',
			status: 'working',
			detail: [
				'212 subagents in 14 days were told they were isolated in a worktree under a Mac home folder, on this Linux machine. The cause is not known yet.'
			],
			links: [{ kind: 'evidence', text: "The engineer's session", href: `/sessions/${ENGINEER_PATHS}` }]
		},
		{
			id: 'p3',
			title: 'Launches still open a day after their seat ended',
			opened: '15 Oct 09:30',
			openedBy: 'Wright',
			status: 'working',
			detail: [
				'121 launches. The fix to the part that should close them is written and waiting for the gate.'
			],
			links: [{ kind: 'evidence', text: "The engineer's session", href: `/sessions/${ENGINEER_LAUNCHES}` }]
		}
	];

	const JUDGE_PROBLEM: Problem = {
		id: 'p4',
		title: 'Judge calls failing',
		opened: 'Today 20:20',
		openedBy: 'Wright',
		status: 'needs-you',
		detail: ['148 of 672 judge calls failed since 20:05 (22%). The hour before: 2%.'],
		links: [{ kind: 'evidence', text: 'Answer in the conversation', href: '#wright-msg-m7' }],
		decisionId: 'd-judge',
		afterAnswer: {
			off: {
				status: 'watching',
				detail: ['The judge is off, as you answered. It goes back on when credit is added.']
			},
			credit: {
				status: 'working',
				detail: ['Checking the next 50 judge calls after you said credit was added.']
			},
			dismiss: {
				status: 'needs-you',
				detail: ['148 of 672 judge calls failed since 20:05 (22%). You dismissed the question.']
			}
		}
	};

	const NORMAL_ACTIONS: Action[] = [
		{
			id: 'a2',
			day: 'Today',
			at: '20:40',
			what: [
				'Landed ',
				{ text: 'afc42e23', href: `${REPO}afc42e23`, external: true },
				' on main: the roster reaper now also releases rows left from earlier boots. Its next pass released 992 stale rows.'
			],
			undo: 'Revert',
			links: []
		},
		{
			id: 'a-forks',
			day: 'Today',
			at: '10:12',
			what: ['Opened a problem, as you asked: forks start without a warm cache.'],
			undo: 'Undo',
			links: [{ kind: 'problem', text: 'Forks start without a warm cache', href: '#wright-problem-p1' }]
		},
		{
			id: 'a3',
			day: 'Today',
			at: '08:15',
			what: ['Cleared 3 alarms whose condition had ended, each with a note saying why.'],
			undo: 'Undo',
			links: []
		},
		{
			id: 'a4',
			day: 'Today',
			at: '07:58',
			what: ['Stopped 2 test containers left running for 3 days by an interrupted test run.'],
			undo: 'Undo',
			links: []
		},
		{
			id: 'a5',
			day: 'Yesterday',
			at: '18:36',
			what: [
				'Bounced a mail from ',
				session(CR3_COORDINATOR, 'coordinator-e74ba661'),
				' to ',
				session(TOOLING_T11, 'tooling-T11'),
				', which closed at 18:31, and told the sender that ',
				session(RUST_DEV_2, 'rust-developer-2'),
				' holds that task now.'
			],
			undo: null,
			noUndo: 'Mail cannot be unsent.',
			links: [{ kind: 'evidence', text: 'The thread', href: `/comms?thread=${BOUNCED_THREAD}` }]
		},
		{
			id: 'a6',
			day: 'Yesterday',
			at: '16:40',
			what: [
				'Landed ',
				{ text: '4c1e9a2', href: `${REPO}4c1e9a2`, external: true },
				' on main: send_session_message fills in who the mail is from when the call leaves it out. 65 calls had failed on that in two weeks.'
			],
			undo: 'Revert',
			links: [{ kind: 'measure', text: '65 failures before, 0 since', href: '/tools?tool_name=send_session_message' }]
		}
	];

	const STEER_ACTION: Action = {
		id: 'a-steer',
		day: 'Today',
		at: '21:10',
		what: [
			'Paused bouncing mail during fleet runs, as you asked. It switches back on when ',
			{ text: 'change-record-4', href: '/fleet' },
			' closes.'
		],
		undo: 'Undo',
		links: [{ kind: 'fix', text: 'Bounce mail sent to a closed seat', href: '#wright-fix-k1' }]
	};

	const NORMAL_FIXES: KnownFix[] = [
		{
			id: 'k5',
			name: 'Clear an alarm whose condition has ended, with a note',
			verdict: 'better',
			measure: ['Alarms left open after their condition ended: 0 of 17 with this fix, 6 of 6 without.'],
			since: 'On since 14 Oct'
		},
		{
			id: 'k4',
			name: 'Stop waking a seat that has nothing to do',
			verdict: 'no-change',
			measure: [
				'Wakes with nothing to do: 22% with this fix, 24% without. The gap could be chance, so it was turned off on 15 Oct.'
			],
			since: 'Ran 12 to 15 Oct'
		},
		{
			id: 'k3',
			name: 'Send a question nobody answered in time to whoever can answer it',
			verdict: 'measuring',
			measure: ['7 of the 30 sessions it needs before it can be judged.'],
			since: 'On since 13 Oct'
		},
		{
			id: 'k2',
			name: 'Hold mail for a seat busy in a long command until the command ends',
			verdict: 'measuring',
			measure: ['18 of the 30 sessions it needs before it can be judged.'],
			since: 'On since 12 Oct'
		},
		{
			id: 'k1',
			name: 'Bounce mail sent to a closed seat, and tell the sender who holds the work',
			verdict: 'better',
			measure: [
				'Mail left waiting for a closed seat: 0 of 41 with this fix, 9 of 12 in the sessions left alone for comparison.'
			],
			since: 'On since 12 Oct'
		}
	];

	const COST: Cost = { spent: '$4.12', problems: 9, cached: '87%' };

	const SCENARIOS: Record<'empty' | 'normal' | 'decision' | 'steer' | 'both', Scenario> = {
		empty: { messages: [], problems: [], actions: [], fixes: [], cost: null },
		normal: {
			messages: NORMAL_MESSAGES,
			problems: NORMAL_PROBLEMS,
			actions: NORMAL_ACTIONS,
			fixes: NORMAL_FIXES,
			cost: COST
		},
		decision: {
			messages: [...NORMAL_MESSAGES, DECISION_MESSAGE],
			problems: [JUDGE_PROBLEM, ...NORMAL_PROBLEMS],
			actions: NORMAL_ACTIONS,
			fixes: NORMAL_FIXES,
			cost: COST
		},
		steer: {
			messages: [...NORMAL_MESSAGES, ...STEER_MESSAGES],
			problems: NORMAL_PROBLEMS,
			actions: [STEER_ACTION, ...NORMAL_ACTIONS],
			fixes: NORMAL_FIXES.map((f) => (f.id === 'k1' ? { ...f, pausedBy: 'a-steer' } : f)),
			cost: COST
		},
		both: {
			messages: [...NORMAL_MESSAGES, ...STEER_MESSAGES, { ...DECISION_MESSAGE, at: '21:14' }],
			problems: [JUDGE_PROBLEM, ...NORMAL_PROBLEMS],
			actions: [STEER_ACTION, ...NORMAL_ACTIONS],
			fixes: NORMAL_FIXES.map((f) => (f.id === 'k1' ? { ...f, pausedBy: 'a-steer' } : f)),
			cost: COST
		}
	};
</script>

<script lang="ts">
	import {
		CircleAlert,
		CirclePlus,
		ExternalLink,
		FileSearch,
		MessageSquare,
		PauseCircle,
		PowerOff,
		Scale,
		Send,
		Undo2,
		Wrench
	} from '@lucide/svelte';
	import type { Component } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { Field } from '$lib/components/ui/field';
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import CommsAvatar from '$lib/comms/CommsAvatar.svelte';

	const LINK_KIND: Record<LinkKind, { label: string; icon: Component }> = {
		problem: { label: 'Problem', icon: CircleAlert },
		action: { label: 'Action', icon: Wrench },
		undo: { label: 'Undo', icon: Undo2 },
		measure: { label: 'Measure', icon: Scale },
		fix: { label: 'Known fix', icon: Wrench },
		evidence: { label: '', icon: FileSearch }
	};

	const CHANGE_ICON: Record<Change['icon'], Component> = {
		pause: PauseCircle,
		open: CirclePlus,
		off: PowerOff
	};

	const STATUS: Record<ProblemStatus, { label: string; variant: BadgeVariant }> = {
		'needs-you': { label: 'Needs you', variant: 'chip-warning' },
		working: { label: 'Working on it', variant: 'chip-analyzed' },
		watching: { label: 'Watching', variant: 'chip-muted' },
		fixed: { label: 'Fixed', variant: 'chip-active' }
	};

	const VERDICT: Record<Verdict, { label: string; variant: BadgeVariant }> = {
		better: { label: 'Better', variant: 'chip-active' },
		'no-change': { label: 'No change · off', variant: 'chip-muted' },
		measuring: { label: 'Measuring', variant: 'chip-analyzed' }
	};

	let undone = $state<Record<string, boolean>>({});
	let answered = $state<Record<string, string>>({});
	let closedBands = $state<Record<string, boolean>>({});
	let draft = $state('');
	let sent = $state<string[]>([]);

	function toggleBand(key: string): void {
		closedBands[key] = !closedBands[key];
	}

	function send(): void {
		const text = draft.trim();
		if (text.length === 0) return;
		sent = [...sent, text];
		draft = '';
	}

	function scrollToEnd(node: HTMLElement): void {
		node.scrollTop = node.scrollHeight;
	}

	function problemView(p: Problem): { status: ProblemStatus; detail: Seg[] } {
		const answer = p.decisionId ? answered[p.decisionId] : undefined;
		const after = answer ? p.afterAnswer?.[answer] : undefined;
		return after ?? { status: p.status, detail: p.detail };
	}

	function dayGroups<T extends { day: string }>(rows: T[]): { day: string; rows: T[] }[] {
		const groups: { day: string; rows: T[] }[] = [];
		for (const row of rows) {
			const last = groups.at(-1);
			if (last && last.day === row.day) last.rows.push(row);
			else groups.push({ day: row.day, rows: [row] });
		}
		return groups;
	}
</script>

{#snippet segs(parts: Seg[])}
	{#each parts as part, i (i)}
		<!-- eslint-disable svelte/no-navigation-without-resolve -- mock hrefs mix in-page anchors, query strings and external commit links; the built components take resolved hrefs from the API layer (PAGE_SPEC.md) -->
		{#if typeof part === 'string'}{part}{:else if part.external}<a
				class="wr-link"
				href={part.href}
				target="_blank"
				rel="noopener noreferrer">{part.text}<ExternalLink size={11} aria-hidden="true" /></a
			>{:else}<a class="wr-link" href={part.href}>{part.text}</a>{/if}
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
	{/each}
{/snippet}

{#snippet chipRow(links: MsgLink[])}
	{#if links.length > 0}
		<ul class="wr-chips" role="list">
			{#each links as link, i (link.kind + i)}
				{@const kind = LINK_KIND[link.kind]}
				{@const Icon = kind.icon}
				<li>
					{#if link.href}
						<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- mock hrefs are in-page anchors and query strings; see segs above -->
						<a class="wr-chip" href={link.href}>
							<Icon size={11} aria-hidden="true" />
							{#if kind.label}<span class="wr-chip__kind">{kind.label}</span>{/if}
							{link.text}
						</a>
					{:else}
						<span class="wr-chip wr-chip--label">
							<Icon size={11} aria-hidden="true" />
							{#if kind.label}<span class="wr-chip__kind">{kind.label}</span>{/if}
							{link.text}
						</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
{/snippet}

{#snippet undoSlot(actionId: string, label: 'Undo' | 'Revert', subject: string)}
	<span class="wr-undo-slot">
		{#if undone[actionId]}
			<span class="wr-undone" role="status">Undone</span>
		{:else}
			<Button
				variant="outline"
				size="sm"
				aria-label="{label}: {subject}"
				onclick={() => (undone[actionId] = true)}
			>
				<Undo2 aria-hidden="true" />
				{label}
			</Button>
		{/if}
	</span>
{/snippet}

{#snippet changeCard(change: Change)}
	{@const Icon = CHANGE_ICON[change.icon]}
	<div class="edge wr-change" class:wr-change--undone={undone[change.actionId]} data-slot="wright-change">
		<div class="wr-change__row">
			<Icon size={16} aria-hidden="true" class="wr-change__icon" />
			<div class="wr-change__text">
				<p class="wr-change__title">{change.title}</p>
				<p class="wr-change__detail">{change.detail}</p>
			</div>
			{@render undoSlot(change.actionId, 'Undo', change.title)}
		</div>
	</div>
{/snippet}

{#snippet decisionCard(d: Decision)}
	{@const answer = answered[d.id]}
	{@const chosen = d.answers.find((a) => a.id === answer)}
	<div class="edge wr-decision" data-slot="wright-decision">
		<div class="wr-decision__inner">
			<p class="wr-decision__question">{d.question}</p>
			{#each d.evidence as para, i (i)}
				<p class="wr-msg__p">{@render segs(para)}</p>
			{/each}
			{@render chipRow(d.links)}
			{#if chosen}
				<p class="wr-decision__chosen" role="status">You chose: {chosen.label}.</p>
				{#if chosen.change}
					{@render changeCard(chosen.change)}
				{:else if chosen.reply}
					<p class="wr-msg__p">{chosen.reply}</p>
				{/if}
			{:else}
				<div class="wr-decision__answers">
					{#each d.answers as a (a.id)}
						<Button variant={a.look} onclick={() => (answered[d.id] = a.id)}>{a.label}</Button>
					{/each}
				</div>
			{/if}
		</div>
	</div>
{/snippet}

{#snippet message(m: Message)}
	<li class="wr-msg" id="wright-msg-{m.id}" data-from={m.from}>
		<div class="wr-msg__gutter">
			{#if m.from === 'wright'}
				<CommsAvatar sessionUuid="wright" label="Wright" sender="mike" />
			{:else}
				<span class="wr-avatar-you" aria-hidden="true">YO</span>
			{/if}
		</div>
		<div class="wr-msg__col">
			<div class="wr-msg__head">
				<span class="wr-msg__sender">{m.from === 'wright' ? 'Wright' : 'You'}</span>
				<span class="wr-msg__time">{m.at}</span>
				{#if m.decision}
					<span class="wr-msg__flag">
						<Badge variant={answered[m.decision.id] ? 'chip-muted' : 'chip-warning'}
							>{answered[m.decision.id] ? 'Answered' : 'Needs you'}</Badge
						>
					</span>
				{/if}
			</div>
			{#each m.body as para, i (i)}
				<p class="wr-msg__p">{@render segs(para)}</p>
			{/each}
			{#if m.decision}{@render decisionCard(m.decision)}{/if}
			{#if m.change}{@render changeCard(m.change)}{/if}
			{@render chipRow(m.links ?? [])}
		</div>
	</li>
{/snippet}

{#snippet bandHeader(key: string, label: string, meta: string)}
	<div class="band-hd-wrap">
		<button
			type="button"
			class="band-hd"
			aria-expanded={!closedBands[key]}
			aria-controls="wright-band-{key}"
			onclick={() => toggleBand(key)}
		>
			<DisclosureMarker open={!closedBands[key]} />
			<span class="machine wr-band-label">{label}</span>
			<span class="band-hd__meta">{meta}</span>
		</button>
	</div>
{/snippet}

{#snippet page(s: Scenario)}
	<div class="wr-scroll">
		<section class="wr" aria-label="Wright">
			<PageIntro
				title="Wright"
				level="h1"
				description="Wright finds what goes wrong in sessions, mail and fleets, fixes it, and tells you what it did. Ask it about anything here, or tell it what to leave alone."
			/>

			<div class="wr-body">
				<section class="wr-convo" aria-labelledby="wr-convo-h">
					<h2 class="machine wr-col-h" id="wr-convo-h">Conversation</h2>
					<div class="edge wr-convo__frame">
						<div class="wr-convo__inner">
							{#if s.messages.length === 0 && sent.length === 0}
								<div class="wr-convo__empty">
									<FilteredEmptyState
										icon={MessageSquare}
										filtered={false}
										emptyText="No conversation yet"
										emptySubtext="Ask why Wright did something, or tell it what to leave alone. It writes here first only when it needs a decision from you."
										filteredText=""
									/>
								</div>
							{:else}
								<ol class="wr-convo__list" use:scrollToEnd aria-label="Conversation with Wright">
									{#each dayGroups(s.messages) as group (group.day)}
										<li class="wr-day" aria-hidden="true">
											<span class="wr-day__label">{group.day}</span>
										</li>
										{#each group.rows as m (m.id)}
											{@render message(m)}
										{/each}
									{/each}
									{#each sent as text, i (i)}
										{@render message({
											id: `sent-${i}`,
											from: 'you',
											day: 'Today',
											at: 'now',
											body: [[text]]
										})}
									{/each}
								</ol>
							{/if}
							<form
								class="wr-compose"
								onsubmit={(e) => {
									e.preventDefault();
									send();
								}}
							>
								<Field
									multiline
									rows={2}
									bind:value={draft}
									aria-label="Message Wright"
									placeholder="Ask Wright, or tell it what to leave alone"
									class="wr-compose__field"
								/>
								<Button
									type="submit"
									size="icon"
									aria-label="Send"
									disabled={draft.trim().length === 0}
								>
									<Send aria-hidden="true" />
								</Button>
							</form>
						</div>
					</div>
				</section>

				<section class="wr-doing" aria-labelledby="wr-doing-h">
					<h2 class="machine wr-col-h" id="wr-doing-h">What Wright is doing</h2>
					<p class="wr-cost">
						{#if s.cost}
							This week: {s.cost.spent} across {s.cost.problems} problems. {s.cost.cached} of what it
							read came from the cache.
						{:else}
							Nothing spent this week.
						{/if}
					</p>

					<div class="edge mech-group wr-stack">
						<div class="wr-band">
							{@render bandHeader('open', 'Open now', `${s.problems.length} open`)}
							{#if !closedBands.open}
								<div class="band-body" id="wright-band-open">
									{#if s.problems.length === 0}
										<p class="wr-none">
											Nothing open. Wright checks every hour, and straight away when something
											fails.
										</p>
									{:else}
										<ul class="wr-rows" role="list">
											{#each s.problems as p (p.id)}
												{@const view = problemView(p)}
												<li class="wr-row" id="wright-problem-{p.id}">
													<div class="wr-row__head">
														<p class="wr-row__title">{p.title}</p>
														<span class="wr-row__badge">
															<Badge variant={STATUS[view.status].variant}
																>{STATUS[view.status].label}</Badge
															>
														</span>
													</div>
													<p class="wr-row__meta">Opened by {p.openedBy} · {p.opened}</p>
													<p class="wr-row__text">{@render segs(view.detail)}</p>
													{@render chipRow(p.links)}
												</li>
											{/each}
										</ul>
									{/if}
								</div>
							{/if}
						</div>

						<div class="wr-band">
							{@render bandHeader('did', 'What it did', `${s.actions.length} this week`)}
							{#if !closedBands.did}
								<div class="band-body" id="wright-band-did">
									{#if s.actions.length === 0}
										<p class="wr-none">Nothing done yet.</p>
									{:else}
										{#each dayGroups(s.actions) as group (group.day)}
											<p class="machine wr-subday">{group.day}</p>
											<ul class="wr-rows" role="list">
												{#each group.rows as a (a.id)}
													<li
														class="wr-row wr-row--action"
														class:wr-row--undone={undone[a.id]}
														id="wright-action-{a.id}"
													>
														<span class="wr-row__time">{a.at}</span>
														<div class="wr-row__main">
															<p class="wr-row__text">{@render segs(a.what)}</p>
															{#if a.noUndo}<p class="wr-row__meta">{a.noUndo}</p>{/if}
															{@render chipRow(a.links)}
														</div>
														{#if a.undo}
															{@render undoSlot(a.id, a.undo, `${a.day} ${a.at}`)}
														{:else}
															<span class="wr-undo-slot"></span>
														{/if}
													</li>
												{/each}
											</ul>
										{/each}
									{/if}
								</div>
							{/if}
						</div>

						<div class="wr-band">
							{@render bandHeader('fixes', 'Known fixes', `${s.fixes.length}`)}
							{#if !closedBands.fixes}
								<div class="band-body" id="wright-band-fixes">
									{#if s.fixes.length === 0}
										<p class="wr-none">
											None yet. Wright adds one each time it solves a problem that will come back.
										</p>
									{:else}
										<ul class="wr-rows" role="list">
											{#each s.fixes as f (f.id)}
												{@const paused = f.pausedBy !== undefined && !undone[f.pausedBy]}
												<li class="wr-row" id="wright-fix-{f.id}">
													<div class="wr-row__head">
														<p class="wr-row__title">{f.name}</p>
														<span class="wr-row__badge">
															{#if paused}
																<Badge variant="chip-warning">Paused by you</Badge>
															{:else}
																<Badge variant={VERDICT[f.verdict].variant}
																	>{VERDICT[f.verdict].label}</Badge
																>
															{/if}
														</span>
													</div>
													<p class="wr-row__text">{@render segs(f.measure)}</p>
													<p class="wr-row__meta">{f.since}</p>
												</li>
											{/each}
										</ul>
									{/if}
								</div>
							{/if}
						</div>
					</div>
				</section>
			</div>
		</section>
	</div>
{/snippet}

<Story
	name="Empty"
	play={async ({ canvas }) => {
		await expect(canvas.getByText('No conversation yet')).toBeVisible();
		await expect(canvas.getByText(/Nothing open\./)).toBeVisible();
		await expect(canvas.getByText('Nothing spent this week.')).toBeVisible();
		await expect(canvas.getByRole('button', { name: 'Send' })).toBeDisabled();
	}}
>
	{#snippet template()}
		{@render page(SCENARIOS.empty)}
	{/snippet}
</Story>

<Story
	name="Normal"
	play={async ({ canvas }) => {
		const open = canvas.getByRole('button', { name: /open now/i });
		await expect(open).toHaveAttribute('aria-expanded', 'true');
		await userEvent.click(open);
		await expect(open).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(open);
		await expect(open).toHaveAttribute('aria-expanded', 'true');

		const box = canvas.getByRole('textbox', { name: 'Message Wright' });
		const sendButton = canvas.getByRole('button', { name: 'Send' });
		await expect(sendButton).toBeDisabled();
		await userEvent.type(box, 'What did the judge outage cost?');
		await expect(sendButton).toBeEnabled();
		await userEvent.clear(box);
		await expect(sendButton).toBeDisabled();
	}}
>
	{#snippet template()}
		{@render page(SCENARIOS.normal)}
	{/snippet}
</Story>

<Story
	name="Decision waiting"
	play={async ({ canvas }) => {
		await expect(
			canvas.getByText('Turn the judge off until you add OpenRouter credit?')
		).toBeVisible();
		for (const name of ['Turn judge off', 'Credit added', 'Dismiss']) {
			await expect(canvas.getByRole('button', { name })).toBeVisible();
		}
		const row = canvas
			.getByText('Judge calls failing', { selector: '.wr-row__title' })
			.closest('li') as HTMLElement;
		await expect(within(row).getByText('Needs you')).toBeVisible();
		await expect(
			within(row).getByRole('link', { name: /Answer in the conversation/ })
		).toHaveAttribute('href', '#wright-msg-m7');
	}}
>
	{#snippet template()}
		{@render page(SCENARIOS.decision)}
	{/snippet}
</Story>

<Story
	name="Steer just applied"
	play={async ({ canvas }) => {
		await expect(canvas.getByText('Paused: bounce mail during fleet runs')).toBeVisible();
		const fixRow = canvas
			.getByText(/Bounce mail sent to a closed seat, and tell/, { selector: '.wr-row__title' })
			.closest('li') as HTMLElement;
		await expect(within(fixRow).getByText('Paused by you')).toBeVisible();
		await expect(
			canvas.getByRole('button', { name: 'Undo: Paused: bounce mail during fleet runs' })
		).toBeEnabled();
		const actionRow = canvas.getByText(/Paused bouncing mail during fleet runs/).closest('li');
		await expect(actionRow).toHaveAttribute('id', 'wright-action-a-steer');
	}}
>
	{#snippet template()}
		{@render page(SCENARIOS.steer)}
	{/snippet}
</Story>

<Story
	name="Narrow"
	play={async ({ canvas }) => {
		const convo = canvas.getByRole('heading', { name: 'Conversation' });
		const doing = canvas.getByRole('heading', { name: 'What Wright is doing' });
		await expect(doing.getBoundingClientRect().top).toBeGreaterThan(
			convo.getBoundingClientRect().bottom
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Turn judge off' }));
		await expect(canvas.getByText('You chose: Turn judge off.')).toBeVisible();
		const row = canvas
			.getByText('Judge calls failing', { selector: '.wr-row__title' })
			.closest('li') as HTMLElement;
		await expect(within(row).getByText('Watching')).toBeVisible();

		const fixRow = canvas
			.getByText(/Bounce mail sent to a closed seat, and tell/, { selector: '.wr-row__title' })
			.closest('li') as HTMLElement;
		await expect(within(fixRow).getByText('Paused by you')).toBeVisible();
		await userEvent.click(
			canvas.getByRole('button', { name: 'Undo: Paused: bounce mail during fleet runs' })
		);
		await waitFor(() => expect(within(fixRow).getByText('Better')).toBeVisible());
		await expect(canvas.getAllByText('Undone').length).toBe(2);
	}}
>
	{#snippet template()}
		<div class="wr-narrow">
			{@render page(SCENARIOS.both)}
		</div>
	{/snippet}
</Story>

<style>
	.wr-narrow {
		inline-size: 390px;
	}

	.wr-scroll {
		min-height: 0;
		background: var(--surface-bg);
		container-type: inline-size;
	}

	.wr {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		max-width: 1600px;
		padding: var(--space-5) var(--space-6) var(--space-8);
	}

	.wr-body {
		display: grid;
		grid-template-columns: minmax(0, 5fr) minmax(22rem, 4fr);
		gap: var(--space-5);
		align-items: start;
	}

	.wr-col-h {
		margin: 0 0 var(--space-2);
		font-size: var(--text-xs);
		color: var(--text-3);
	}

	.wr-convo {
		position: sticky;
		top: var(--space-4);
		min-width: 0;
	}

	.wr-convo__frame {
		--edge: var(--edge-sm);
		--edge-pad: 0px;
		--edge-line: var(--border-subtle);
		--edge-surface: var(--surface-card);
	}

	.wr-convo__inner {
		display: flex;
		flex-direction: column;
		block-size: min(46rem, calc(100svh - 12rem));
		min-block-size: 24rem;
	}

	.wr-convo__list {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		margin: 0;
		padding: var(--space-2) var(--space-4);
		list-style: none;
	}

	.wr-convo__empty {
		flex: 1;
		display: grid;
		place-items: center;
		padding: var(--space-5);
	}

	.wr-day {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		margin: var(--space-2) 0;
	}

	.wr-day::before,
	.wr-day::after {
		content: '';
		flex: 1;
		height: 1px;
		background: var(--border-subtle);
	}

	.wr-day__label,
	.wr-subday {
		font-family: var(--font-mono);
		font-size: var(--text-micro);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.wr-msg {
		display: flex;
		gap: var(--space-2);
		padding: var(--space-3) 0;
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.wr-day + .wr-msg {
		border-top: none;
	}

	.wr-msg__gutter {
		flex: none;
		width: var(--control-h);
		padding-top: 2px;
	}

	.wr-avatar-you {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: var(--control-h);
		height: var(--control-h);
		border-radius: 4px;
		border: 1px solid var(--ident-session-line);
		background: var(--ident-session-fill);
		color: var(--ident-session-fg);
		font-family: var(--font-mono);
		font-size: 13px;
		font-weight: 600;
	}

	.wr-msg__col {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.wr-msg__head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-2);
	}

	.wr-msg__sender {
		font-family: var(--font-display);
		font-size: var(--text-md);
		font-weight: 700;
		color: var(--text-strong);
	}

	.wr-msg__time {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.wr-msg__flag {
		margin-left: auto;
	}

	.wr-msg__p {
		margin: 0;
		max-width: 92ch;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.wr-link {
		display: inline;
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.wr-link :global(svg) {
		display: inline;
		margin-left: 2px;
		vertical-align: -1px;
	}

	.wr-link:focus-visible,
	.wr-chip:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.wr-chips {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.wr-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 7px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono);
		font-size: 10px;
		text-decoration: none;
	}

	a.wr-chip:hover {
		border-color: var(--interactive-active-border);
		color: var(--text-strong);
	}

	.wr-chip--label {
		color: var(--text-3);
	}

	.wr-chip__kind {
		white-space: nowrap;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.wr-decision {
		--edge: var(--edge-sm);
		--edge-line: var(--status-warning-border);
		--edge-surface: var(--surface-2);
	}

	.wr-decision__inner {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-1);
	}

	.wr-decision__question {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-md);
		font-weight: 700;
		line-height: var(--leading-tight);
		color: var(--text-strong);
	}

	.wr-decision__answers {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		padding-top: var(--space-1);
	}

	.wr-decision__chosen {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-2);
	}

	.wr-change {
		--edge: var(--edge-sm);
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-2);
	}

	.wr-change__row {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.wr-change__row :global(.wr-change__icon) {
		flex: none;
		color: var(--signal-active);
	}

	.wr-change--undone .wr-change__row :global(.wr-change__icon),
	.wr-change--undone .wr-change__title {
		color: var(--text-3);
	}

	.wr-change__text {
		flex: 1;
		min-width: 0;
	}

	.wr-change__title {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
	}

	.wr-change__detail {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.wr-undo-slot {
		flex: none;
		order: 9;
		margin-left: auto;
		display: inline-flex;
		justify-content: flex-end;
		inline-size: 6.5rem;
	}

	.wr-undone {
		display: inline-flex;
		align-items: center;
		block-size: 1.75rem;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.wr-compose {
		display: flex;
		align-items: flex-end;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.wr-compose :global(.wr-compose__field) {
		flex: 1;
	}

	.wr-doing {
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.wr-cost {
		margin: 0 0 var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-2);
	}

	.wr-stack {
		--edge-line: var(--border-subtle);
		--edge-surface: var(--surface-card);
	}

	.wr-band-label {
		margin: 0;
		font-size: var(--text-xs);
		color: inherit;
	}

	.wr-none {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--text-2);
	}

	.wr-subday {
		margin: var(--space-2) 0 0;
	}

	.wr-rows {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.wr-row {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-2);
		scroll-margin-top: var(--space-6);
	}

	.wr-row + .wr-row {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.wr-row:target {
		background: var(--surface-2);
	}

	.wr-row--action {
		flex-direction: row;
		align-items: flex-start;
		gap: var(--space-3);
	}

	.wr-row--undone .wr-row__text {
		color: var(--text-3);
	}

	.wr-row__head {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.wr-row__title {
		flex: 1;
		min-width: 0;
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
	}

	.wr-row__badge {
		flex: none;
		order: 9;
		margin-left: auto;
	}

	.wr-row__time {
		flex: none;
		inline-size: 3rem;
		padding-top: 2px;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.wr-row__main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.wr-row__text {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.wr-row__meta {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	@container (max-width: 860px) {
		.wr {
			padding: var(--space-4) var(--space-4) var(--space-6);
		}

		.wr-body {
			grid-template-columns: minmax(0, 1fr);
		}

		.wr-convo {
			position: static;
		}

		.wr-convo__inner {
			block-size: 70svh;
		}

		.wr-row--action {
			flex-wrap: wrap;
			row-gap: var(--space-2);
		}

		.wr-row__main {
			flex-basis: calc(100% - 3rem - var(--space-3));
		}

		.wr-change__row {
			flex-wrap: wrap;
		}

		.wr-change__text {
			flex-basis: calc(100% - 16px - var(--space-2));
		}
	}
</style>
