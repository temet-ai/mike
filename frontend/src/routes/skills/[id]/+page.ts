import type { PageLoad } from './$types';
import { getProjectSkills, getSkillDraft } from '$lib/api/skills';
import type { SkillDraftResponse, SkillFoldersResponse } from '$lib/api/skills';

export interface SkillDraftPageData {
	id: number;
	detail: SkillDraftResponse | null;
	/** The project's skill folders, which the reviewer's advice links into; null when they could not be read. */
	skills: SkillFoldersResponse | null;
	error: string | null;
}

export const load: PageLoad = async ({ params }): Promise<SkillDraftPageData> => {
	const id = Number(params.id);
	if (!Number.isInteger(id) || id <= 0) {
		return { id, detail: null, skills: null, error: `"${params.id}" is not a skill draft number` };
	}
	let detail: SkillDraftResponse;
	try {
		detail = await getSkillDraft(id);
	} catch (e) {
		return { id, detail: null, skills: null, error: e instanceof Error ? e.message : String(e) };
	}
	let skills: SkillFoldersResponse | null = null;
	try {
		skills = await getProjectSkills(detail.draft.project_uuid);
	} catch {
		skills = null;
	}
	return { id, detail, skills, error: null };
};
