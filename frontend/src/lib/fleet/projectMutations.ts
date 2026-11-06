import { patchProjectPin, patchProjectName, mergeProjectInto, createProject } from '$lib/api';
import type { FleetProjectWithCounts } from './types';

export interface ProjectListAccess {
	get: () => FleetProjectWithCounts[];
	set: (next: FleetProjectWithCounts[]) => void;
}

export async function renameProject(
	list: ProjectListAccess,
	path: string,
	customName: string | null
): Promise<void> {
	const idx = list.get().findIndex((p) => p.project_path === path);
	if (idx === -1) return;
	const previous = list.get()[idx]!.custom_name;
	list.set(list.get().map((p, i) => (i === idx ? { ...p, custom_name: customName } : p)));
	try {
		const res = await patchProjectName(path, customName);
		list.set(
			list.get().map((p) => (p.project_path === path ? { ...p, custom_name: res.custom_name } : p))
		);
	} catch (err) {
		list.set(
			list.get().map((p) => (p.project_path === path ? { ...p, custom_name: previous } : p))
		);
		throw err;
	}
}

export async function mergeProject(
	list: ProjectListAccess,
	path: string,
	intoProjectUuid: string
): Promise<string> {
	const res = await mergeProjectInto(path, intoProjectUuid);
	list.set(
		list
			.get()
			.filter((p) => p.project_path !== path)
			.map((p) =>
				p.project_uuid === res.target_uuid
					? { ...p, session_count: p.session_count + res.sessions_moved }
					: p
			)
	);
	return res.target_path;
}

export async function createProjectFor(
	list: ProjectListAccess,
	path: string,
	name: string
): Promise<string> {
	const source = list.get().find((p) => p.project_path === path);
	const created = await createProject(name);
	const res = await mergeProjectInto(path, created.project_uuid);
	const joined: FleetProjectWithCounts = {
		project_path: res.target_path,
		project_uuid: res.target_uuid,
		project_id: created.id,
		session_count: res.sessions_moved,
		request_count: source?.request_count ?? 0,
		first_seen: source?.first_seen ?? new Date().toISOString(),
		last_seen: source?.last_seen ?? new Date().toISOString(),
		pinned: false,
		custom_name: name
	};
	list.set([...list.get().filter((p) => p.project_path !== path), joined]);
	return joined.project_path;
}

export async function togglePin(
	list: ProjectListAccess,
	path: string,
	next: boolean
): Promise<void> {
	const idx = list.get().findIndex((p) => p.project_path === path);
	if (idx === -1) return;
	const previous = list.get()[idx]!.pinned;
	list.set(list.get().map((p, i) => (i === idx ? { ...p, pinned: next } : p)));
	try {
		const res = await patchProjectPin(path, next);
		list.set(list.get().map((p) => (p.project_path === path ? { ...p, pinned: res.pinned } : p)));
	} catch {
		list.set(list.get().map((p) => (p.project_path === path ? { ...p, pinned: previous } : p)));
	}
}
