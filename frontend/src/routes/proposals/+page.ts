import type { PageLoad } from './$types';
import { getProposalProjects, getProposals } from '$lib/api/proposals';
import type { ProposalProject, ProposalsResponse } from '$lib/api/proposals';
import { ORDER_PARAM, orderFromParam } from '$lib/suggestedOrder';
import type { ListOrder } from '$lib/suggestedOrder';

export interface ProposalsPageData {
	projects: ProposalProject[] | null;
	projectsError: string | null;
	selected: string | null;
	proposals: ProposalsResponse | null;
	proposalsError: string | null;
	order: ListOrder;
}

function message(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

function pick(projects: ProposalProject[], asked: string | null): string | null {
	if (asked && projects.some((p) => p.project_uuid === asked)) return asked;
	return (projects.find((p) => p.undecided > 0) ?? projects[0])?.project_uuid ?? null;
}

export const load: PageLoad = async ({ url }): Promise<ProposalsPageData> => {
	const order = orderFromParam(url.searchParams.get(ORDER_PARAM));
	let projects: ProposalProject[];
	try {
		projects = (await getProposalProjects()).projects;
	} catch (e) {
		return {
			projects: null,
			projectsError: message(e),
			selected: null,
			proposals: null,
			proposalsError: null,
			order
		};
	}
	const selected = pick(projects, url.searchParams.get('project'));
	if (selected === null) {
		return {
			projects,
			projectsError: null,
			selected,
			proposals: null,
			proposalsError: null,
			order
		};
	}
	try {
		const proposals = await getProposals(selected, order);
		return { projects, projectsError: null, selected, proposals, proposalsError: null, order };
	} catch (e) {
		return {
			projects,
			projectsError: null,
			selected,
			proposals: null,
			proposalsError: message(e),
			order
		};
	}
};
