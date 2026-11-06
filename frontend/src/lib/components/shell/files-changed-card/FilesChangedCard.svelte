<script lang="ts">
	import { untrack } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { ChevronsDownUp, ChevronsUpDown, FileCode2, Folder, FolderOpen } from '@lucide/svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import FileTypeIcon from './FileTypeIcon.svelte';

	interface Props {
		files: readonly string[];
		loading?: boolean;
		error?: string | null;
		onRetry: () => void;
	}

	interface FileNode {
		kind: 'file';
		name: string;
		path: string;
	}

	interface DirectoryNode {
		kind: 'directory';
		name: string;
		path: string;
		children: TreeNode[];
	}

	type TreeNode = DirectoryNode | FileNode;

	interface MutableDirectory {
		name: string;
		path: string;
		directories: Map<string, MutableDirectory>;
		files: FileNode[];
	}

	interface DirectoryRow {
		kind: 'directory';
		node: DirectoryNode;
		depth: number;
		connectsBefore: boolean;
		connectsAfter: boolean;
	}

	interface FileRow {
		kind: 'file';
		node: FileNode;
		depth: number;
	}

	interface SeparatorRow {
		kind: 'separator';
		depth: number;
	}

	type TreeRow = DirectoryRow | FileRow | SeparatorRow;

	const INITIAL_ROW_BUDGET = 16;

	let { files, loading = false, error = null, onRetry }: Props = $props();
	let expansion = new SvelteMap<string, boolean>();

	function mutableDirectory(name: string, path: string): MutableDirectory {
		return { name, path, directories: new Map(), files: [] };
	}

	function compactDirectory(node: DirectoryNode): DirectoryNode {
		let name = node.name;
		let path = node.path;
		let children = node.children;

		while (children.length === 1 && children[0]?.kind === 'directory') {
			const child = children[0];
			name = `${name}/${child.name}`;
			path = child.path;
			children = child.children;
		}

		return { kind: 'directory', name, path, children };
	}

	function finaliseDirectory(directory: MutableDirectory): DirectoryNode {
		const childDirectories = Array.from(directory.directories.values())
			.sort((a, b) => a.name.localeCompare(b.name))
			.map(finaliseDirectory);
		const childFiles = directory.files.toSorted((a, b) => a.name.localeCompare(b.name));

		return compactDirectory({
			kind: 'directory',
			name: directory.name,
			path: directory.path,
			children: [...childDirectories, ...childFiles]
		});
	}

	function buildFileTree(filePaths: readonly string[]): TreeNode[] {
		const root = mutableDirectory('', '');

		for (const filePath of new Set(filePaths)) {
			const parts = filePath.split('/').filter(Boolean);
			if (parts.length === 0) continue;

			let current = root;
			for (const [index, part] of parts.entries()) {
				if (index === parts.length - 1) {
					current.files.push({ kind: 'file', name: part, path: filePath });
					continue;
				}

				const directoryPath = current.path ? `${current.path}/${part}` : part;
				let next = current.directories.get(part);
				if (!next) {
					next = mutableDirectory(part, directoryPath);
					current.directories.set(part, next);
				}
				current = next;
			}
		}

		const directories = Array.from(root.directories.values())
			.sort((a, b) => a.name.localeCompare(b.name))
			.map(finaliseDirectory);
		const rootFiles = root.files.toSorted((a, b) => a.name.localeCompare(b.name));
		return [...directories, ...rootFiles];
	}

	function appendRows(
		nodes: readonly TreeNode[],
		depth: number,
		collapsed: ReadonlySet<string>,
		rows: TreeRow[]
	): void {
		const directories = nodes.filter((node): node is DirectoryNode => node.kind === 'directory');
		const childFiles = nodes.filter((node): node is FileNode => node.kind === 'file');

		for (const [index, directory] of directories.entries()) {
			if (index > 0) rows.push({ kind: 'separator', depth });
			rows.push({
				kind: 'directory',
				node: directory,
				depth,
				connectsBefore: false,
				connectsAfter: false
			});

			if (!collapsed.has(directory.path)) {
				appendRows(directory.children, depth + 1, collapsed, rows);
			}
		}

		if (directories.length > 0 && childFiles.length > 0) {
			rows.push({ kind: 'separator', depth });
		}

		for (const file of childFiles) {
			rows.push({ kind: 'file', node: file, depth });
		}
	}

	function flattenVisibleTree(
		nodes: readonly TreeNode[],
		collapsed: ReadonlySet<string>
	): TreeRow[] {
		const rows: TreeRow[] = [];
		appendRows(nodes, 0, collapsed, rows);

		for (let index = 0; index < rows.length - 1; index += 1) {
			const current = rows[index];
			const next = rows[index + 1];
			if (
				current?.kind === 'directory' &&
				next?.kind === 'directory' &&
				next.depth === current.depth + 1 &&
				next.node.path.startsWith(`${current.node.path}/`)
			) {
				current.connectsAfter = true;
				next.connectsBefore = true;
			}
		}

		return rows;
	}

	function directoryLabel(directory: DirectoryNode): string {
		const segments = directory.name.split('/');
		if (directory.name.length <= 48 || segments.length <= 3) return directory.name;

		return `${segments[0]}/${segments[1]}/.../${segments.at(-1)}`;
	}

	function seedCollapsed(nodes: readonly TreeNode[], into: Map<string, boolean>): void {
		for (const node of nodes) {
			if (node.kind !== 'directory') continue;
			into.set(node.path, false);
			seedCollapsed(node.children, into);
		}
	}

	function defaultExpansion(nodes: readonly TreeNode[]): Map<string, boolean> {
		const expanded = new Map<string, boolean>();
		seedCollapsed(nodes, expanded);

		let rows = nodes.length;
		const queue = nodes.filter((node): node is DirectoryNode => node.kind === 'directory');

		while (queue.length > 0) {
			const directory = queue.shift();
			if (!directory) break;
			if (rows + directory.children.length > INITIAL_ROW_BUDGET) continue;

			expanded.set(directory.path, true);
			rows += directory.children.length;
			for (const child of directory.children) {
				if (child.kind === 'directory') queue.push(child);
			}
		}

		return expanded;
	}

	function isExpanded(path: string): boolean {
		return expansion.get(path) ?? defaults.get(path) ?? true;
	}

	function toggleDirectory(path: string): void {
		expansion.set(path, !isExpanded(path));
	}

	function setAllExpanded(expanded: boolean): void {
		for (const path of defaults.keys()) expansion.set(path, expanded);
	}

	let fileTree = $derived(buildFileTree(files));
	let defaults = $derived(defaultExpansion(fileTree));

	$effect(() => {
		const opening = defaults;
		untrack(() => {
			for (const [path, expanded] of opening) {
				if (!expansion.has(path)) expansion.set(path, expanded);
			}
		});
	});

	let collapsedDirectories = $derived(
		new Set(Array.from(defaults.keys()).filter((path) => !isExpanded(path)))
	);
	let treeRows = $derived(flattenVisibleTree(fileTree, collapsedDirectories));
	let hasCollapsed = $derived(collapsedDirectories.size > 0);
	let hasDirectories = $derived(defaults.size > 0);
</script>

<div class="fc-card edge edge-blue">
	<div class="fc-card-heading panel-hd">
		<FileCode2 size={13} aria-hidden="true" />
		Files changed <span class="fc-total">{files.length}</span>
		{#if !loading && !error && hasDirectories}
			<div class="fc-tree-actions">
				<button
					type="button"
					class="fc-tree-action"
					title="Expand all"
					aria-label="Expand all folders"
					disabled={!hasCollapsed}
					onclick={() => setAllExpanded(true)}
				>
					<ChevronsUpDown size={13} aria-hidden="true" />
				</button>
				<button
					type="button"
					class="fc-tree-action"
					title="Collapse all"
					aria-label="Collapse all folders"
					disabled={collapsedDirectories.size === defaults.size}
					onclick={() => setAllExpanded(false)}
				>
					<ChevronsDownUp size={13} aria-hidden="true" />
				</button>
			</div>
		{/if}
	</div>

	{#if error}
		<LoadErrorBanner
			title="Couldn't load tool uses"
			detail={error}
			hint="Files changed and agents active are derived from the same fetch."
			{onRetry}
		/>
	{:else if loading}
		<div class="fc-skeleton" data-motion="wait" aria-hidden="true"></div>
		<div class="fc-skeleton" data-motion="wait" aria-hidden="true"></div>
		<div class="fc-skeleton fc-skeleton--short" data-motion="wait" aria-hidden="true"></div>
	{:else if files.length === 0}
		<div class="fc-empty">No files changed</div>
	{:else}
		<ul class="fc-file-tree" role="tree" aria-label="Changed files">
			{#each treeRows as row}
				{#if row.kind === 'directory'}
					<li
						class="fc-tree-row fc-tree-row--directory"
						class:fc-tree-row--connected-before={row.connectsBefore}
						class:fc-tree-row--connected-after={row.connectsAfter}
						style={`--tree-indent: ${row.depth * 14}px`}
						role="none"
					>
						<button
							type="button"
							class="fc-directory-button"
							role="treeitem"
							aria-level={row.depth + 1}
							aria-selected="false"
							aria-expanded={isExpanded(row.node.path)}
							aria-label={`${isExpanded(row.node.path) ? 'Collapse' : 'Expand'} folder ${row.node.path}`}
							data-folder-path={row.node.path}
							data-folder-icon={isExpanded(row.node.path) ? 'open' : 'closed'}
							title={row.node.path}
							onclick={() => toggleDirectory(row.node.path)}
						>
							{#if !isExpanded(row.node.path)}
								<Folder size={13} aria-hidden="true" />
							{:else}
								<FolderOpen size={13} aria-hidden="true" />
							{/if}
							<span class="fc-directory-label">{directoryLabel(row.node)}</span>
						</button>
					</li>
				{:else if row.kind === 'file'}
					<li
						class="fc-tree-row fc-file-row"
						style={`--tree-indent: ${row.depth * 14}px`}
						role="treeitem"
						aria-level={row.depth + 1}
						aria-selected="false"
						aria-label={row.node.path}
						data-path={row.node.path}
						title={row.node.path}
					>
						<FileTypeIcon path={row.node.path} size={13} class="fc-file-icon" />
						<span class="fc-file-name">{row.node.name}</span>
					</li>
				{:else}
					<li
						class="fc-tree-separator"
						style={`--tree-indent: ${row.depth * 14}px`}
						aria-hidden="true"
					></li>
				{/if}
			{/each}
		</ul>
	{/if}
</div>

<style>
	.fc-card {
		--edge: 10px;
		min-width: 0;
	}

	.fc-card-heading {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 8px;
	}

	.fc-total {
		display: inline-flex;
		align-items: center;
		padding: 1px 6px;
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-2);
		font-size: 10px;
		font-variant-numeric: tabular-nums;
	}

	.fc-tree-actions {
		display: flex;
		gap: 2px;
		margin-left: auto;
	}

	.fc-tree-action {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		padding: 0;
		border: 1px solid var(--border-default);
		border-radius: 2px;
		background: var(--surface-bg);
		color: var(--text-3);
		cursor: pointer;
	}

	.fc-tree-action:hover:not(:disabled) {
		border-color: var(--border-strong);
		color: var(--text-1);
	}

	.fc-tree-action:disabled {
		color: var(--text-faint);
		cursor: default;
	}

	.fc-tree-action:focus-visible {
		outline: 1px solid var(--interactive-active-border);
		outline-offset: 1px;
	}

	.fc-file-tree {
		display: flex;
		flex-direction: column;
		min-width: 0;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.fc-tree-row {
		min-width: 0;
	}

	.fc-tree-row + .fc-tree-row {
		margin-top: 3px;
	}

	.fc-tree-row--connected-before {
		margin-top: 0 !important;
	}

	.fc-directory-button {
		display: grid;
		grid-template-columns: 13px minmax(0, 1fr);
		gap: 6px;
		align-items: center;
		width: calc(100% - var(--tree-indent));
		min-height: 21px;
		margin: 0 0 0 var(--tree-indent);
		padding: 3px 6px;
		border: 0;
		border-radius: 2px;
		background: color-mix(in oklch, var(--surface-card) 84%, var(--surface-bg) 16%);
		color: var(--text-2);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.fc-directory-button:hover {
		background: color-mix(in oklch, var(--surface-card) 72%, var(--surface-2) 28%);
		color: var(--text-1);
	}

	.fc-directory-button:focus-visible {
		outline: 1px solid var(--interactive-active-border);
		outline-offset: 1px;
	}

	.fc-tree-row--connected-after .fc-directory-button {
		border-bottom-right-radius: 0;
		border-bottom-left-radius: 0;
	}

	.fc-tree-row--connected-before .fc-directory-button {
		border-top-left-radius: 0;
		border-top-right-radius: 0;
		box-shadow: inset 1px 0 var(--border-subtle);
	}

	.fc-directory-button :global(svg) {
		color: var(--text-3);
	}

	.fc-directory-label,
	.fc-file-name {
		min-width: 0;
		font-family: var(--font-mono-condensed);
		font-size: 11px;
		line-height: 1.35;
		overflow-wrap: anywhere;
	}

	.fc-file-row {
		display: grid;
		grid-template-columns: 13px minmax(0, 1fr);
		align-items: start;
		gap: 6px;
		min-height: 18px;
		padding: 1px 5px 1px calc(var(--tree-indent) + 6px);
		color: var(--text-1);
	}

	.fc-file-row :global(svg.fc-file-icon) {
		margin-top: 1px;
	}

	.fc-tree-separator {
		height: 1px;
		margin: 4px 3px 3px calc(var(--tree-indent) + 19px);
		border-top: 1px solid var(--border-subtle);
	}

	.fc-empty {
		color: var(--text-3);
		font-size: 11px;
	}

	.fc-skeleton {
		height: 12px;
		margin: 4px 0;
		border-radius: 2px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
	}

	.fc-skeleton--short {
		width: 55%;
	}
</style>
