export const NO_PROJECT = 'No project';

export interface ConversationItem {
	projectLabel: string;
	projectPath?: string | null;
	sessionHref: string;
	sessionTitle: string;
}

export interface Conversation<T> {
	href: string;
	title: string;
	items: T[];
}

export interface ProjectGroup<T> {
	projectLabel: string;
	projectPath: string | null;
	count: number;
	conversations: Conversation<T>[];
}

/** The last folder of a project path, or "No project" when there is none. */
export function projectLabelOf(path: string | null | undefined): string {
	if (path === null || path === undefined || path === '(no project)') return NO_PROJECT;
	const last = path
		.split('/')
		.filter((part) => part !== '')
		.at(-1);
	return last ?? NO_PROJECT;
}

function compareProjectLabels(a: string, b: string): number {
	if (a === NO_PROJECT) return b === NO_PROJECT ? 0 : 1;
	if (b === NO_PROJECT) return -1;
	return a.localeCompare(b, undefined, { sensitivity: 'base' });
}

/**
 * Projects alphabetically ("No project" last), then the conversations inside each in the order
 * their first item appears, items kept in input order. Callers sort by an immutable time first.
 */
export function groupByProjectAndConversation<T extends ConversationItem>(
	items: readonly T[]
): ProjectGroup<T>[] {
	const projects = new Map<string, ProjectGroup<T>>();
	for (const item of items) {
		let project = projects.get(item.projectLabel);
		if (!project) {
			project = {
				projectLabel: item.projectLabel,
				projectPath: item.projectPath ?? null,
				count: 0,
				conversations: []
			};
			projects.set(item.projectLabel, project);
		}
		project.count += 1;
		let conversation = project.conversations.find((c) => c.href === item.sessionHref);
		if (!conversation) {
			conversation = { href: item.sessionHref, title: item.sessionTitle, items: [] };
			project.conversations.push(conversation);
		}
		conversation.items.push(item);
	}
	return [...projects.values()].sort((a, b) =>
		compareProjectLabels(a.projectLabel, b.projectLabel)
	);
}
