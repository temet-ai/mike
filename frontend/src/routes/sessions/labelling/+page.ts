import type { PageLoad } from './$types';
import { loadJudgePage } from '$lib/components/judges/judgePageData';
import type { JudgePageData } from '$lib/components/judges/judgePageData';

export const load: PageLoad = ({ url }): Promise<JudgePageData> =>
	loadJudgePage(url.searchParams.get('for') === 'sessions' ? 'session_judge' : 'turn_judge', url);
