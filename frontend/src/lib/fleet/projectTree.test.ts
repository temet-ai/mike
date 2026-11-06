import { describe, it, expect } from 'vitest';
import {
	buildProjectTree,
	flattenVisibleProjectTree,
	folderLabel,
	isUnknownProjectPath,
	pruneVisibleProjectTree,
	type ProjectFolderNode,
	type ProjectLeafNode
} from './projectTree';
import type { FleetProjectWithCounts } from './types';

function project(
	overrides: Partial<FleetProjectWithCounts> & { project_path: string }
): FleetProjectWithCounts {
	return {
		session_count: 1,
		request_count: 1,
		first_seen: '2026-01-01T00:00:00Z',
		last_seen: '2026-01-01T00:00:00Z',
		pinned: false,
		custom_name: null,
		...overrides
	};
}

describe('isUnknownProjectPath', () => {
	it('classifies the no-project sentinel', () => {
		expect(isUnknownProjectPath('(no project)')).toBe(true);
	});

	it('classifies a UUID path segment', () => {
		expect(isUnknownProjectPath('d309b653-5065-466a-bf4b-9f82bff2fd49/scratchpad/spawn-test')).toBe(
			true
		);
	});

	it('classifies a mktemp-style tmp. segment', () => {
		expect(isUnknownProjectPath('2hhvhg9s7fg5f80flxq139d80000gn/T/tmp.qXWuJ926d2')).toBe(true);
	});

	it('classifies a long opaque alnum-with-digit segment (macOS $TMPDIR component)', () => {
		expect(isUnknownProjectPath('2hhvhg9s7fg5f80flxq139d80000gn/T/some-project')).toBe(true);
	});

	it('does not classify an ordinary multi-segment project path', () => {
		expect(isUnknownProjectPath('projects/temet/temet-proxy')).toBe(false);
	});

	it('does not classify a short alnum segment even with digits', () => {
		expect(isUnknownProjectPath('work/ankh-morpork/night-watch2026')).toBe(false);
	});

	it('does not classify a long alnum segment with no digits', () => {
		expect(isUnknownProjectPath('projects/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/repo')).toBe(false);
	});
});

describe('buildProjectTree', () => {
	it('compacts a single-child folder chain through to its terminal leaf, not stopping one level short', () => {
		const tree = buildProjectTree([project({ project_path: 'work/ankh-morpork/night-watch' })]);
		expect(tree).toHaveLength(1);
		const leaf = tree[0] as ProjectLeafNode;
		expect(leaf.kind).toBe('leaf');
		expect(leaf.name).toBe('work/ankh-morpork/night-watch');
		expect(leaf.path).toBe('work/ankh-morpork/night-watch');
	});

	it('compacts each single-child branch under a real fork into its own leaf row', () => {
		const tree = buildProjectTree([
			project({ project_path: 'repositories/am-supplier-product/supplier-product-catalog' }),
			project({ project_path: 'repositories/am-ledger-clerk/ledger-clerk' })
		]);
		expect(tree).toHaveLength(1);
		const repositories = tree[0] as ProjectFolderNode;
		expect(repositories.kind).toBe('folder');
		expect(repositories.name).toBe('repositories');
		expect(repositories.children).toHaveLength(2);
		expect(repositories.children.every((child) => child.kind === 'leaf')).toBe(true);
		expect(repositories.children.map((child) => child.name)).toEqual([
			'am-ledger-clerk/ledger-clerk',
			'am-supplier-product/supplier-product-catalog'
		]);
	});

	it('rolls up session/request counts and the max last_seen across descendant leaves', () => {
		const tree = buildProjectTree([
			project({
				project_path: 'projects/temet/temet-proxy',
				session_count: 40,
				request_count: 100,
				last_seen: '2026-07-16T08:00:00Z'
			}),
			project({
				project_path: 'projects/temet/temet-aide',
				session_count: 9,
				request_count: 20,
				last_seen: '2026-07-10T08:00:00Z'
			})
		]);
		const folder = tree[0] as ProjectFolderNode;
		expect(folder.name).toBe('projects/temet');
		expect(folder.sessionCount).toBe(49);
		expect(folder.requestCount).toBe(120);
		expect(folder.lastSeen).toBe('2026-07-16T08:00:00Z');
	});

	it('sorts folders and leaves alphabetically, folders before leaves at the same level', () => {
		const tree = buildProjectTree([
			project({ project_path: 'zeta-project' }),
			project({ project_path: 'alpha/beta' }),
			project({ project_path: 'alpha-project' })
		]);
		expect(tree.map((node) => node.name)).toEqual(['alpha/beta', 'alpha-project', 'zeta-project']);
	});
});

describe('pruneVisibleProjectTree', () => {
	it('keeps the folder and the unpinned leaf when only one sibling is pinned (no compaction)', () => {
		const tree = buildProjectTree([
			project({ project_path: 'projects/temet/temet-proxy', pinned: true }),
			project({ project_path: 'projects/temet/temet-aide', pinned: false })
		]);
		const pruned = pruneVisibleProjectTree(tree);
		expect(pruned).toHaveLength(1);
		const folder = pruned[0] as ProjectFolderNode;
		expect(folder.kind).toBe('folder');
		expect(folder.name).toBe('projects/temet');
		expect(folder.children).toHaveLength(1);
		expect(folder.children[0]?.kind).toBe('leaf');
		expect((folder.children[0] as ProjectLeafNode).name).toBe('temet-aide');
	});

	it('drops a folder entirely once every descendant leaf is pinned', () => {
		const tree = buildProjectTree([
			project({ project_path: 'ankh-morpork/ai/tasker', pinned: true }),
			project({ project_path: 'ankh-morpork/ai/svc-coord-eval-f0', pinned: true })
		]);
		expect(pruneVisibleProjectTree(tree)).toEqual([]);
	});

	it('keeps rollup counts summed over pinned + unpinned members even after pruning', () => {
		const tree = buildProjectTree([
			project({
				project_path: 'projects/temet/temet-proxy',
				pinned: true,
				session_count: 40,
				request_count: 100
			}),
			project({
				project_path: 'projects/temet/temet-aide',
				pinned: false,
				session_count: 9,
				request_count: 20
			})
		]);
		const folder = pruneVisibleProjectTree(tree)[0] as ProjectFolderNode;
		expect(folder.sessionCount).toBe(49);
		expect(folder.requestCount).toBe(120);
	});

	it('is a no-op when nothing is pinned', () => {
		const tree = buildProjectTree([
			project({ project_path: 'projects/temet/temet-proxy' }),
			project({ project_path: 'projects/temet/temet-aide' })
		]);
		expect(pruneVisibleProjectTree(tree)).toEqual(tree);
	});
});

describe('flattenVisibleProjectTree', () => {
	it('omits children of a collapsed folder', () => {
		const tree = buildProjectTree([
			project({ project_path: 'projects/temet/temet-proxy' }),
			project({ project_path: 'projects/temet/temet-aide' })
		]);
		const folder = tree[0] as ProjectFolderNode;

		const expandedRows = flattenVisibleProjectTree(tree, new Set());
		expect(expandedRows.filter((row) => row.kind === 'leaf')).toHaveLength(2);

		const collapsedRows = flattenVisibleProjectTree(tree, new Set([folder.path]));
		expect(collapsedRows).toHaveLength(1);
		expect(collapsedRows[0]?.kind).toBe('folder');
	});
});

describe('folderLabel', () => {
	it('returns the name unchanged when short', () => {
		const folder: ProjectFolderNode = {
			kind: 'folder',
			name: 'projects/temet',
			path: 'projects/temet',
			children: [],
			sessionCount: 0,
			requestCount: 0,
			lastSeen: '2026-01-01T00:00:00Z'
		};
		expect(folderLabel(folder)).toBe('projects/temet');
	});

	it('elides the middle of a very long compacted chain', () => {
		const folder: ProjectFolderNode = {
			kind: 'folder',
			name: 'aaaaaaaaaaaa/bbbbbbbbbbbb/cccccccccccc/dddddddddddd/eeeeeeeeeeee',
			path: 'aaaaaaaaaaaa/bbbbbbbbbbbb/cccccccccccc/dddddddddddd/eeeeeeeeeeee',
			children: [],
			sessionCount: 0,
			requestCount: 0,
			lastSeen: '2026-01-01T00:00:00Z'
		};
		expect(folderLabel(folder)).toBe('aaaaaaaaaaaa/bbbbbbbbbbbb/.../eeeeeeeeeeee');
	});
});
