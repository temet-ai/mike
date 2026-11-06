import type { Proposal } from '$lib/api/proposals';

export interface ProposalTarget {
	/** The file the change belongs in, relative to the project folder. */
	path: string;
	/** True when no such file exists yet and the change creates it. */
	creates: boolean;
	/** The text to add to that file. */
	change: string;
}

function slug(topic: string): string {
	return topic
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '');
}

function title(topic: string): string {
	const words = topic.replace(/[-_]+/g, ' ').trim();
	return words.charAt(0).toUpperCase() + words.slice(1);
}

/**
 * Where a rule or agent proposal goes and what it adds. A file the placement named is used as it
 * is; otherwise a rule goes in a new .claude/rules file named after the lesson's topic, and an
 * agent statement goes in that agent's definition.
 */
export function proposalTarget(p: Proposal): ProposalTarget {
	const line = `- ${p.text}`;
	if (p.covered_by) return { path: p.covered_by, creates: false, change: line };
	if (p.placement === 'agent') {
		const path = p.agent_name ? `.claude/agents/${p.agent_name}.md` : '.claude/agents/<agent>.md';
		return { path, creates: false, change: line };
	}
	const name = p.topic ? slug(p.topic) : '';
	if (!name) return { path: `.claude/rules/lesson-${p.lesson_id}.md`, creates: true, change: line };
	return {
		path: `.claude/rules/${name}.md`,
		creates: true,
		change: `# ${title(p.topic ?? name)}\n\n${line}\n`
	};
}

/** What happens after the user's decision, in one sentence. */
export function proposalOutcome(p: Proposal, target: ProposalTarget): string {
	if (p.decision === 'accepted') {
		return `Accepted. mike does not write rule or agent files yet, so add the text above to ${target.path} yourself.`;
	}
	if (p.decision === 'declined') return 'Declined. It stays a lesson and is not added to any file.';
	return p.placement === 'rule'
		? 'Waiting for your decision. mike never adds a rule to your project on its own.'
		: 'Waiting for your decision. mike never changes an agent definition on its own.';
}
