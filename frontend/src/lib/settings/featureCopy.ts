export interface FeatureCopy {
	label: string;
	description: string;
	/** Words a search should also match. Never shown. */
	find?: string;
}

export const FEATURE_COPY: Record<string, FeatureCopy> = {
	reminders: {
		label: 'Project reminders',
		description: "Adds the project's lessons and reminders to the prompt at the start of each turn."
	},
	recall_injector: {
		label: 'Recall from project memory',
		description: 'Adds earlier turns from project memory that resemble the current one.'
	},
	reminder_on_token: {
		label: 'Resolved-answer reminders',
		description: 'When a search matches a known question, attaches its resolved answer.'
	},
	mailbox_redelivery: {
		label: 'Mailbox redelivery',
		description:
			'Keeps attaching a message from another session until it is acknowledged. Off, each message is delivered once.'
	},
	native_peer_mail: {
		label: 'Mail as messages from other sessions',
		description:
			"Delivers mail from another session the way Claude Code delivers a message from one of your other sessions: a one-line preview naming the sender, and the full text for Claude with the sender's name and an address to reply to. Off, mail arrives as a mike channel message. Applies to the next message.",
		find: 'cross-session native peer SendMessage inbox socket mail channel'
	},
	mod_lesson_delivery: {
		label: 'Lessons delivered by the mike mod',
		description:
			'The mike mod adds the lessons that fit a prompt when the prompt is sent, tags each with its lesson number, and Claude Code keeps them in the session from then on. mike stops adding lessons to a session once it holds tagged ones. One session in five keeps mike adding lessons, so the two ways can be compared; the memory.lesson_delivery_arms table records which way each session got. Applies to the next prompt, and only to sessions that have the mike mod loaded.',
		find: 'lessons hook context transcript mod prompt submit control group experiment'
	},
	repeated_notices: {
		label: 'Repeated notices',
		description:
			'Claude Code adds the same notice to every message another session or mike sends to a session, and the same closing sentence to every message you send while a session is working. When on, a session is sent each of them once and later copies are removed before a request is sent. New sessions pick this up when they start; running ones when the proxy restarts. The full original is always captured.',
		find: 'disclaimer permission laundering not from your user mid-turn message wrapper'
	},
	decisions: {
		label: 'Jev decisions',
		description:
			"Asks Jev (typesafe/jev-1.14 on OpenRouter) narrow typed questions at mike's decision points, starting with whether a turn-guard nudge is worth sending. It sends the operator's ask, the end of the assistant's final message and the facts about what changed to OpenRouter. Each point has its own mode under Decisions (off, shadow or on); in shadow the answers are only recorded. When a call fails or there is no OpenRouter key, mike behaves as if this were off. Applies to the next decision.",
		find: 'jev typesafe openrouter pre-check precheck shadow hold nudge'
	},
	change_record: {
		label: 'Record of what changed',
		description:
			'Keeps one record of what happens in each checkout a session is working in: every file change with the session that made it and what the file held before, every commit and push, and the notes, mail and check results that name those files. Nothing a session sends or receives is changed. Off, nothing is recorded.',
		find: 'what changed git status file watcher who edited history'
	},
	client_serves_self_identity: {
		label: 'tether adds the session id reminder',
		description:
			"tether adds the session's id and the mike tools to use it with; mike stops adding them for that session. Off, mike adds them.",
		find: 'tether client serves stage self identity'
	},
	client_serves_workspace_briefing: {
		label: 'tether adds the workspace briefing',
		description:
			"tether adds the session's peers and its own name, role and goal; mike stops adding them for that session. Off, mike adds them.",
		find: 'tether client serves stage briefing roster'
	},
	client_serves_change_record_tail: {
		label: 'tether says what others changed',
		description:
			'tether adds what other sessions changed since the session last looked; mike stops adding it for that session. Off, mike adds it.',
		find: 'tether client serves stage change tail'
	},
	client_serves_reminders: {
		label: 'tether adds analyzer findings',
		description:
			"tether adds the findings from the session's last answer; mike stops adding them for that session. Off, mike adds them.",
		find: 'tether client serves stage reminders findings diagnostics'
	},
	client_serves_recall: {
		label: 'tether answers recall requests',
		description:
			'tether adds the lessons for a topic the model asks to recall; mike stops adding them for that session. Off, mike adds them.',
		find: 'tether client serves stage recall mem'
	},
	client_serves_agent_lessons: {
		label: 'tether gives subagents their lessons',
		description:
			'tether gives a subagent its agent lessons when it starts; mike stops adding them for that session. Off, mike adds them.',
		find: 'tether client serves stage agent lessons subagent'
	},
	client_serves_prompt_shaping: {
		label: 'tether applies filter rules and compositions',
		description:
			"tether applies the project's filter rules, lazy sections and compositions; mike stops rewriting that session's requests. Off, mike rewrites them.",
		find: 'tether client serves stage filter lazy sections compositions'
	},
	client_serves_proxy_tools: {
		label: 'tether offers the session roster tool',
		description:
			'tether offers the roster tool and relays its calls to mike; mike stops adding the tool for that session. Off, mike adds it.',
		find: 'tether client serves stage proxy tools roster'
	},
	client_serves_weasel_guard: {
		label: 'tether guards the words',
		description:
			"tether cuts the session's answers at the forbidden words; mike stops intercepting that session's answers. Off, mike intercepts them.",
		find: 'tether client serves stage weasel intercept word guard'
	},
	client_serves_mail: {
		label: 'tether delivers mail and launch briefs',
		description:
			"tether delivers the session's mike mail and launch briefs; mike stops adding them to that session's requests. Off, mike adds them.",
		find: 'tether client serves stage mail mailbox launch brief'
	},
	client_serves_git_read_hint: {
		label: 'tether gives the git read hint',
		description:
			"After a session reads git status, diff or log, tether points it at what other sessions changed; mike stops adding that hint to the session's requests. Off, mike adds it.",
		find: 'tether client serves stage git read hint what changed'
	},
	client_serves_memory_keeper: {
		label: 'tether gives the work thread card',
		description:
			'tether adds the card of the work thread a prompt continues, and mike stops adding it for that session. No effect until card delivery ships.',
		find: 'tether client serves stage memory keeper thread card'
	},
	memory_keeper: {
		label: 'Work thread cards',
		description:
			'Keeps a short card for each open work thread: where it stands, what is next, blockers, the last handoff, decisions since, open promises and unmet asks. Built from the database on one machine; nothing leaves the machine. Turning it off stops the builder; turning it back on needs a restart.',
		find: 'memory keeper thread card continue work thread'
	},
	change_record_tail: {
		label: 'Say what changed on answers',
		description:
			'Adds a few lines saying what other sessions changed since a session last looked to the answers it already gets when it commits or asks for a check. Needs the record of what changed. Off, answers are exactly as they were.',
		find: 'since you last looked what changed tail'
	},
	code_scan: {
		label: 'Scan edited code',
		description:
			'Scans each file a session changes with the built-in ast-grep rules and records what the change added or fixed. Needs the record of what changed; nothing leaves the machine.',
		find: 'mike-code ast-grep scan edit findings rules'
	},
	code_findings: {
		label: 'Tell sessions what their edit added',
		description:
			'Adds a short note naming each warning or error a session’s own edit just added, with the fix when the rule has one. Off, findings are only recorded.',
		find: 'mike-code ast-grep findings reminder fix'
	},
	file_read_compression: {
		label: 'File-read compression',
		description:
			'Shortens large file reads in the session history before a request is sent. The full original is always captured.'
	},
	shell_output_compression: {
		label: 'Shell-output compression',
		description:
			'Shortens large Bash output in the session history before a request is sent. The full original is always captured.',
		find: 'bash shell command output terminal'
	},
	context_focus_question: {
		label: 'Reason recorded with each tool call',
		description:
			"Asks the model to add one short line to every tool call except file edits, saying what it wants from the call. mike adds a context_focus_question field to the session's tool schemas, records each answer with its tool call, and removes the field before Claude Code sees the call. Costs a few output tokens per call. New sessions pick this up when they start.",
		find: 'cfq focus question intent why tool call reason schema'
	},
	proxy_tools: {
		label: 'Mike tools in every session',
		description:
			"Gives mike's own tools to a session that does not already have them through the mike MCP server.",
		find: 'mike_session_roster roster'
	},
	dossier_builder: {
		label: 'Dossier builder',
		description: 'Builds the facts about each turn from captured data. Nothing leaves the machine.'
	},
	seat_memory_index: {
		label: 'Fleet seat work in knowledge search',
		description:
			'Knowledge search also finds fleet seat briefs, reports and mail, labelled by seat and run and ranked below your own material; turning it back on needs a restart.',
		find: 'seat fleet worker report brief mail search index'
	},
	semantic_context: {
		label: 'Semantic context',
		description:
			'Lets mike find earlier corrections that resemble a turn, using the local embedding model. Nothing leaves the machine.'
	},
	fleet_auto_start: {
		label: 'Start fleets on their own',
		description:
			'When on, mike starts a fleet as soon as its description is complete. When off, it waits for you to run mike fleet start.'
	},
	agent_lessons: {
		label: 'Agent lessons',
		description: "Adds lessons tied to a sub-agent to that sub-agent's prompts."
	}
};

export type OtherPageHref = '/settings/steering' | '/settings/workers' | '/settings/flagging';
export interface OtherPageFeature extends FeatureCopy {
	page: string;
	href: OtherPageHref;
}

// Switches that live on another settings page. They stay off this page until
// a search matches one, so a search here still finds every switch.
export const OTHER_PAGE_FEATURES: Record<string, OtherPageFeature> = {
	steerer: {
		label: 'Steerer — acts on live sessions',
		description:
			'Checks each turn of an enrolled session and nudges it back while it is still running. Needs an OpenRouter key.',
		page: 'Steering',
		href: '/settings/steering'
	},
	weasel_intercept: {
		label: 'Live correction',
		description:
			'Cuts a reply where a listed phrase appears and continues it with a corrected answer.',
		find: 'weasel forbidden words',
		page: 'Flagging',
		href: '/settings/flagging'
	},
	lesson_worker: {
		label: 'Lesson worker',
		description:
			'Merges lessons found in sessions into project memory and removes duplicates. Runs on this machine.',
		page: 'Workers',
		href: '/settings/workers'
	},
	harvester: {
		label: 'Harvester',
		description: 'Finds lessons in finished turns: corrections, conventions and warnings.',
		page: 'Workers',
		href: '/settings/workers'
	},
	db_resync: {
		label: 'Local DB resync',
		description:
			'Copies rows captured locally while the remote database was unreachable back into it. Runs once at startup.',
		page: 'Workers',
		href: '/settings/workers'
	}
};
