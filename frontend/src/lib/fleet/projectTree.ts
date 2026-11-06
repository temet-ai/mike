import type { FleetProjectWithCounts } from './types';

const UNKNOWN_SENTINEL = '(no project)';
const UUID_SEGMENT_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const TMP_PREFIX_RE = /^tmp\./i;
const OPAQUE_TOKEN_RE = /^[a-z0-9]{20,}$/i;

function isSyntheticSegment(segment: string): boolean {
	if (UUID_SEGMENT_RE.test(segment)) return true;
	if (TMP_PREFIX_RE.test(segment)) return true;
	return OPAQUE_TOKEN_RE.test(segment) && /[0-9]/.test(segment);
}

export function isUnknownProjectPath(projectPath: string): boolean {
	if (projectPath === UNKNOWN_SENTINEL) return true;
	return projectPath.split('/').filter(Boolean).some(isSyntheticSegment);
}

export interface ProjectLeafNode {
	kind: 'leaf';
	name: string;
	path: string;
	project: FleetProjectWithCounts;
}

export interface ProjectFolderNode {
	kind: 'folder';
	name: string;
	path: string;
	children: ProjectTreeNode[];
	sessionCount: number;
	requestCount: number;
	lastSeen: string;
}

export type ProjectTreeNode = ProjectFolderNode | ProjectLeafNode;

interface MutableFolder {
	name: string;
	path: string;
	folders: Map<string, MutableFolder>;
	leaves: ProjectLeafNode[];
}

function mutableFolder(name: string, path: string): MutableFolder {
	return { name, path, folders: new Map(), leaves: [] };
}

function newerIso(a: string, b: string): string {
	return new Date(a).getTime() >= new Date(b).getTime() ? a : b;
}

function rollupOf(node: ProjectTreeNode): {
	sessionCount: number;
	requestCount: number;
	lastSeen: string;
} {
	if (node.kind === 'leaf') {
		return {
			sessionCount: node.project.session_count,
			requestCount: node.project.request_count,
			lastSeen: node.project.last_seen
		};
	}
	return {
		sessionCount: node.sessionCount,
		requestCount: node.requestCount,
		lastSeen: node.lastSeen
	};
}

function compactFolder(node: ProjectFolderNode): ProjectTreeNode {
	let name = node.name;
	let path = node.path;
	let children = node.children;

	while (children.length === 1 && children[0]?.kind === 'folder') {
		const child = children[0];
		name = `${name}/${child.name}`;
		path = child.path;
		children = child.children;
	}

	if (children.length === 1 && children[0]?.kind === 'leaf') {
		const leaf = children[0];
		return { ...leaf, name: `${name}/${leaf.name}` };
	}

	return { ...node, name, path, children };
}

function finalizeFolder(folder: MutableFolder): ProjectTreeNode {
	const childFolders = Array.from(folder.folders.values())
		.sort((a, b) => a.name.localeCompare(b.name))
		.map(finalizeFolder);
	const childLeaves = folder.leaves.toSorted((a, b) => a.name.localeCompare(b.name));
	const children: ProjectTreeNode[] = [...childFolders, ...childLeaves];

	let sessionCount = 0;
	let requestCount = 0;
	let lastSeen = new Date(0).toISOString();
	for (const child of children) {
		const rollup = rollupOf(child);
		sessionCount += rollup.sessionCount;
		requestCount += rollup.requestCount;
		lastSeen = newerIso(lastSeen, rollup.lastSeen);
	}

	return compactFolder({
		kind: 'folder',
		name: folder.name,
		path: folder.path,
		children,
		sessionCount,
		requestCount,
		lastSeen
	});
}

export function buildProjectTree(projects: readonly FleetProjectWithCounts[]): ProjectTreeNode[] {
	const root = mutableFolder('', '');

	for (const project of projects) {
		const parts = project.project_path.split('/').filter(Boolean);
		if (parts.length === 0) continue;

		let current = root;
		for (const [index, part] of parts.entries()) {
			if (index === parts.length - 1) {
				current.leaves.push({ kind: 'leaf', name: part, path: project.project_path, project });
				continue;
			}

			const folderPath = current.path ? `${current.path}/${part}` : part;
			let next = current.folders.get(part);
			if (!next) {
				next = mutableFolder(part, folderPath);
				current.folders.set(part, next);
			}
			current = next;
		}
	}

	const folders = Array.from(root.folders.values())
		.sort((a, b) => a.name.localeCompare(b.name))
		.map(finalizeFolder);
	const leaves = root.leaves.toSorted((a, b) => a.name.localeCompare(b.name));
	return [...folders, ...leaves];
}

export function pruneVisibleProjectTree(nodes: readonly ProjectTreeNode[]): ProjectTreeNode[] {
	const kept: ProjectTreeNode[] = [];
	for (const node of nodes) {
		if (node.kind === 'leaf') {
			if (!node.project.pinned) kept.push(node);
			continue;
		}
		const children = pruneVisibleProjectTree(node.children);
		if (children.length === 0) continue;
		kept.push({ ...node, children });
	}
	return kept;
}

export interface ProjectFolderRow {
	kind: 'folder';
	node: ProjectFolderNode;
	depth: number;
	connectsBefore: boolean;
	connectsAfter: boolean;
}

export interface ProjectLeafRow {
	kind: 'leaf';
	node: ProjectLeafNode;
	depth: number;
}

export interface ProjectSeparatorRow {
	kind: 'separator';
	depth: number;
}

export type ProjectTreeRow = ProjectFolderRow | ProjectLeafRow | ProjectSeparatorRow;

function appendProjectRows(
	nodes: readonly ProjectTreeNode[],
	depth: number,
	collapsed: ReadonlySet<string>,
	rows: ProjectTreeRow[]
): void {
	const folders = nodes.filter((node): node is ProjectFolderNode => node.kind === 'folder');
	const leaves = nodes.filter((node): node is ProjectLeafNode => node.kind === 'leaf');

	for (const [index, folder] of folders.entries()) {
		if (index > 0) rows.push({ kind: 'separator', depth });
		rows.push({ kind: 'folder', node: folder, depth, connectsBefore: false, connectsAfter: false });

		if (!collapsed.has(folder.path)) {
			appendProjectRows(folder.children, depth + 1, collapsed, rows);
		}
	}

	if (folders.length > 0 && leaves.length > 0) rows.push({ kind: 'separator', depth });

	for (const leaf of leaves) {
		rows.push({ kind: 'leaf', node: leaf, depth });
	}
}

export function flattenVisibleProjectTree(
	nodes: readonly ProjectTreeNode[],
	collapsedFolders: ReadonlySet<string>
): ProjectTreeRow[] {
	const rows: ProjectTreeRow[] = [];
	appendProjectRows(nodes, 0, collapsedFolders, rows);

	for (let index = 0; index < rows.length - 1; index += 1) {
		const current = rows[index];
		const next = rows[index + 1];
		if (
			current?.kind === 'folder' &&
			next?.kind === 'folder' &&
			next.depth === current.depth + 1 &&
			next.node.path.startsWith(`${current.node.path}/`)
		) {
			current.connectsAfter = true;
			next.connectsBefore = true;
		}
	}

	return rows;
}

export function folderLabel(folder: ProjectFolderNode): string {
	const segments = folder.name.split('/');
	if (folder.name.length <= 48 || segments.length <= 3) return folder.name;

	return `${segments[0]}/${segments[1]}/.../${segments.at(-1)}`;
}

export function projectMatchesQuery(
	project: Pick<FleetProjectWithCounts, 'custom_name' | 'project_path'>,
	normalizedQuery: string
): boolean {
	if (normalizedQuery.length === 0) return true;
	const haystack = `${project.custom_name ?? ''} ${project.project_path}`.toLowerCase();
	return haystack.includes(normalizedQuery);
}
