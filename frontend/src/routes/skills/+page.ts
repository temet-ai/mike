import type { PageLoad } from './$types';
import { getProjectSkills, getSkillProjects } from '$lib/api/skills';
import type { SkillFoldersResponse, SkillProject } from '$lib/api/skills';
import { ORDER_PARAM, orderFromParam } from '$lib/suggestedOrder';
import type { ListOrder } from '$lib/suggestedOrder';

export interface SkillsPageData {
	projects: SkillProject[] | null;
	projectsError: string | null;
	selected: string | null;
	skills: SkillFoldersResponse | null;
	skillsError: string | null;
	order: ListOrder;
}

function message(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

function pick(projects: SkillProject[], asked: string | null): string | null {
	if (asked && projects.some((p) => p.project_uuid === asked)) return asked;
	return (projects.find((p) => p.waiting > 0) ?? projects[0])?.project_uuid ?? null;
}

export const load: PageLoad = async ({ url }): Promise<SkillsPageData> => {
	const order = orderFromParam(url.searchParams.get(ORDER_PARAM));
	let projects: SkillProject[];
	try {
		projects = (await getSkillProjects()).projects;
	} catch (e) {
		return {
			projects: null,
			projectsError: message(e),
			selected: null,
			skills: null,
			skillsError: null,
			order
		};
	}
	const selected = pick(projects, url.searchParams.get('project'));
	if (selected === null) {
		return { projects, projectsError: null, selected, skills: null, skillsError: null, order };
	}
	try {
		const skills = await getProjectSkills(selected, order);
		return { projects, projectsError: null, selected, skills, skillsError: null, order };
	} catch (e) {
		return {
			projects,
			projectsError: null,
			selected,
			skills: null,
			skillsError: message(e),
			order
		};
	}
};
