import type { PageLoad } from './$types';
import { getLessonDetail } from '$lib/api/improvement';
import type { LessonDetail } from '$lib/api/improvement';

export interface LessonDetailPageData {
	id: number;
	lesson: LessonDetail | null;
	error: string | null;
}

export const load: PageLoad = async ({ params }): Promise<LessonDetailPageData> => {
	const id = Number(params.id);
	if (!Number.isInteger(id) || id <= 0) {
		return { id, lesson: null, error: `"${params.id}" is not a lesson number` };
	}
	try {
		const lesson = await getLessonDetail(id);
		return { id, lesson, error: null };
	} catch (e) {
		return { id, lesson: null, error: e instanceof Error ? e.message : String(e) };
	}
};
