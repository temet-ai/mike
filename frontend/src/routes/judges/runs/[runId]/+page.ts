import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ params, url }) => {
	redirect(308, `/sessions/labelling/accuracy/${encodeURIComponent(params.runId)}${url.search}`);
};
