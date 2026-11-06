const KIND_LABELS: Record<string, string> = {
	subagent: 'Subagents',
	gate: 'Gate runs',
	'rust tests': 'Rust tests',
	'frontend tests': 'Frontend tests',
	lint: 'Lint',
	'rust build': 'Rust builds',
	deploy: 'Deploys',
	'waiting on background': 'Waiting on background tasks',
	'database query': 'Database queries',
	git: 'Git',
	'other shell': 'Other shell commands',
	'mcp tool': 'MCP tools',
	'read, edit, search': 'Reading, editing and searching files'
};

const COMMAND_KINDS: ReadonlySet<string> = new Set([
	'gate',
	'rust tests',
	'frontend tests',
	'lint',
	'rust build',
	'deploy',
	'waiting on background',
	'database query',
	'git',
	'other shell'
]);

const BUILD_LABELS: Record<string, string> = {
	none: 'None',
	'1 to 2': '1 to 2 others',
	'3 to 5': '3 to 5 others',
	'6 or more': '6 or more others'
};

export function kindLabel(kind: string): string {
	return KIND_LABELS[kind] ?? kind;
}

export function kindRunsShellCommands(kind: string): boolean {
	return COMMAND_KINDS.has(kind);
}

export function buildLabel(runningAtOnce: string): string {
	return BUILD_LABELS[runningAtOnce] ?? runningAtOnce;
}
