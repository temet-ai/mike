import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import { listEvalRuns } from '$lib/api/evalRuns';
import { getSettings } from '$lib/api/settings';
import { liveIdFrom } from '$lib/components/judges/judgePageData';
import { latestReviewableRun } from './latest';

export const load: PageLoad = async ({ url }) => {
	let target = '/sessions/labelling/accuracy';
	try {
		const [runs, settings] = await Promise.all([
			listEvalRuns({ role: 'turn_judge', limit: 200 }),
			getSettings().catch(() => null)
		]);
		const live = liveIdFrom(settings?.settings ?? null, 'turn_judge');
		const run = latestReviewableRun(runs.items, live);
		if (run) target = `/sessions/labelling/accuracy/${run.id}/review${url.search}`;
	} catch {
		target = '/sessions/labelling/accuracy';
	}
	redirect(307, target);
};
