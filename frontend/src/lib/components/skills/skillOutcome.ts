import type { DraftReview, DraftStatus, SkillMode } from '$lib/api/skills';

export const PROMOTER_SCHEDULE = 'The promoter runs hourly from 20:30 to 03:30 UTC.';

/** What the next promoter run will do with this draft, in one or two sentences. */
export function skillOutcome(
	mode: SkillMode,
	status: DraftStatus,
	review: DraftReview | null,
	retired: boolean
): string {
	const said = ((): string => {
		if (retired) {
			return 'Retired. The next promoter run removes this skill from every checkout, and it stays out until you restore it.';
		}
		if (review === 'approved') {
			return "Approved. The next promoter run writes this draft into the project's checkouts.";
		}
		if (review === 'rejected') return 'Rejected. mike never writes this draft into the project.';
		if (mode === 'ask') {
			return status === 'accepted'
				? 'Passed review. It is written into the project only after you approve it.'
				: 'Not written into the project unless you approve it.';
		}
		return status === 'accepted'
			? 'Passed review. The next promoter run writes it into the project unless you reject it.'
			: 'This project ships reviewed skills on its own. This draft has not passed review, so it is written only if you approve it.';
	})();
	return `${said} ${PROMOTER_SCHEDULE}`;
}
