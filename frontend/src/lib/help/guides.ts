export interface Guide {
	slug: string;
	title: string;
	description: string;
	audience: string;
	duration: string;
}

export const GUIDES: Guide[] = [
	{
		slug: 'getting-started',
		title: 'Get Claude Code talking to mike',
		description:
			'Start the installed app, use its actual connection addresses, and verify a captured request.',
		audience: 'first-time setup',
		duration: '3 min'
	},
	{
		slug: 'inspecting-requests',
		title: 'Inspect a captured request',
		description:
			'Walk through the request detail view: messages, system prompt, parameters, headers, response, and findings.',
		audience: 'operator',
		duration: '4 min'
	},
	{
		slug: 'filter-rules',
		title: 'Rewrite outbound system prompts',
		description:
			'Add, edit, and order filter rules — drop, replace, or append spans of the system block before they reach Anthropic.',
		audience: 'operator',
		duration: '5 min'
	},
	{
		slug: 'anomalies',
		title: 'Investigate a prompt anomaly',
		description:
			'Open the Anomalies tab, read the sub-pattern hint, and turn a captured anomaly into a filter rule in one click.',
		audience: 'operator',
		duration: '4 min'
	},
	{
		slug: 'retention',
		title: 'Keep the data you care about (TTL & pinning)',
		description:
			'Configure global, per-project, and per-session TTL; pin sessions or projects to exempt them from eviction.',
		audience: 'operator',
		duration: '3 min'
	},
	{
		slug: 'steering-concepts',
		title: 'How steering works',
		description:
			'What steerers are, the evaluation lifecycle, authority levels, mode, and enrolment explained with examples.',
		audience: 'operator',
		duration: '5 min'
	}
];
