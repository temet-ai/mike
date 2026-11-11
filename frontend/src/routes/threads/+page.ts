import type { PageLoad } from './$types';
import { listProjects } from '$lib/api';
import { getThread, listThreads } from '$lib/api/workThreads';
import type { ThreadDetail, ThreadListResponse } from '$lib/api/workThreads';
import type { ProjectSummaryDto } from '$lib/types';
import { pickProject } from '$lib/components/threads/threadFormat';

export interface ThreadsPageData {
	projects: ProjectSummaryDto[] | null;
	projectsError: string | null;
	project: string | null;
	status: string;
	list: ThreadListResponse | null;
	listError: string | null;
	slug: string | null;
	detail: ThreadDetail | null;
	detailError: string | null;
}

function message(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

export const load: PageLoad = async ({ url }): Promise<ThreadsPageData> => {
	const status = url.searchParams.get('status') ?? '';
	const empty: ThreadsPageData = {
		projects: null,
		projectsError: null,
		project: null,
		status,
		list: null,
		listError: null,
		slug: null,
		detail: null,
		detailError: null
	};
	let projects: ProjectSummaryDto[];
	try {
		projects = await listProjects();
	} catch (e) {
		return { ...empty, projectsError: message(e) };
	}
	const project = pickProject(projects, url.searchParams.get('project'));
	if (project === null) return { ...empty, projects };
	let list: ThreadListResponse;
	try {
		list = await listThreads(project, status || undefined);
	} catch (e) {
		return { ...empty, projects, project, listError: message(e) };
	}
	const slug = url.searchParams.get('thread');
	if (!slug) return { ...empty, projects, project, list };
	try {
		const detail = await getThread(project, slug);
		return { ...empty, projects, project, list, slug, detail };
	} catch (e) {
		return { ...empty, projects, project, list, slug, detailError: message(e) };
	}
};
