export const REPO_BASE = 'https://github.com/temet-ai/mike';

export const INSTALLED_SETUP_DOC_HREF = '/help/docs/installed-setup';
export const CONNECTIONS_DOC_HREF = '/help/docs/connections-and-privacy';
export const RECOVERY_DOC_HREF = '/help/docs/recovery-and-maintenance';

export interface ExternalDoc {
	label: string;
	href: string;
	description: string;
	group: 'project' | 'anthropic';
	internal?: boolean;
}

export const EXTERNAL_DOCS: ExternalDoc[] = [
	{
		label: 'Installed setup',
		href: INSTALLED_SETUP_DOC_HREF,
		description: 'Configure an installed copy, choose persistence, and make a first capture.',
		group: 'project',
		internal: true
	},
	{
		label: 'Configuration and import',
		href: '/help/docs/configuration-and-import',
		description:
			'Select private configuration, import legacy dotenv values, and distinguish live settings.',
		group: 'project',
		internal: true
	},
	{
		label: 'Connections and privacy',
		href: CONNECTIONS_DOC_HREF,
		description:
			'Wrapper, standalone proxy, MCP, browser, capture requirements, and trust boundaries.',
		group: 'project',
		internal: true
	},
	{
		label: 'Recovery and maintenance',
		href: RECOVERY_DOC_HREF,
		description: 'Troubleshoot capture, restart safely, protect history, and report a problem.',
		group: 'project',
		internal: true
	},
	{
		label: 'Threads',
		href: '/help/docs/threads',
		description: 'Work threads per project: state, acceptance criteria, history and editing.',
		group: 'project',
		internal: true
	},
	{
		label: 'Decisions',
		href: '/help/docs/decisions',
		description: 'Decision points: the switch, modes, the track-record table and the pre-check.',
		group: 'project',
		internal: true
	},
	{
		label: 'Anthropic API docs',
		href: 'https://docs.anthropic.com/',
		description: 'API reference for the upstream Anthropic service.',
		group: 'anthropic'
	},
	{
		label: 'Claude Code overview',
		href: 'https://docs.claude.com/en/docs/claude-code/overview',
		description: 'Getting-started for the Claude Code CLI.',
		group: 'anthropic'
	}
];

export const PROJECT_DOCS: ExternalDoc[] = EXTERNAL_DOCS.filter((d) => d.group === 'project');

export const ANTHROPIC_DOCS: ExternalDoc[] = EXTERNAL_DOCS.filter((d) => d.group === 'anthropic');
