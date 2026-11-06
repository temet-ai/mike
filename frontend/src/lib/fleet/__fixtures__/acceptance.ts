import type { FleetAcceptanceStatus } from '../types';

export function acceptanceStatusFixture(): FleetAcceptanceStatus {
	return {
		fleet: 'acceptance-pilot',
		slug: 'acceptance-pilot',
		state: 'stopped',
		run_id: 'run-7f21',
		goal: 'Dependent work can proceed while the delivered user journey still needs acceptance.',
		now: '2026-10-04T14:30:00Z',
		accepted: false,
		tasks: [
			{
				id: 'T1',
				name: 'Make the setting reachable from command search',
				state: 'done',
				criteria: ['C1', 'C2'],
				passed_criteria: ['C2'],
				ready: true
			}
		],
		criteria: [
			{
				id: 'C1',
				text: 'Search opens the requested setting in context.',
				tasks: 1,
				passed: 0,
				ready: 1
			},
			{
				id: 'C2',
				text: 'Navigation uses the existing settings registry.',
				tasks: 1,
				passed: 1,
				ready: 1
			}
		],
		blocked: [],
		needs_operator: [],
		obligations: [
			{
				note_id: '00000000-0000-4000-8000-000000000101',
				obligation_id: 'setting-journey',
				title: 'Reach the setting through command search',
				body: 'Worker build and test evidence is recorded. The acceptance scenario awaits the served build.',
				data: {
					expected:
						'Selecting the command search result opens the requested setting and keeps it visible.',
					observed:
						'The running app is still serving the preceding build; this scenario has not exercised the change.',
					evidence: ['Worker check: build-104', 'Running build: preceding-release'],
					environment: 'http://localhost:9990',
					owner: 'coordinator',
					next_action: 'Serve the worker build and assign a fresh journey acceptance session.',
					trigger: 'The served build matches the worker artifact.'
				},
				tasks: ['T1'],
				criteria: ['C1'],
				outcome: 'not_exercised',
				required: true,
				issue: null,
				at: '2026-10-04T14:25:00Z',
				session_uuid: '00000000-0000-4000-8000-000000000201'
			},
			{
				note_id: '00000000-0000-4000-8000-000000000102',
				obligation_id: 'setting-ownership',
				title: 'Keep settings navigation in the existing registry',
				body: 'The architecture review followed the search result resolver into the shared settings registry.',
				data: {
					expected:
						'Search and Settings resolve the same setting identifier through the existing registry.',
					observation:
						'Both callers use the existing registry; no competing route lookup was added.',
					evidence: ['Architecture observation note: registry-review']
				},
				tasks: ['T1'],
				criteria: ['C2'],
				outcome: 'accepted',
				required: true,
				issue: null,
				at: '2026-10-04T14:20:00Z',
				session_uuid: '00000000-0000-4000-8000-000000000202'
			},
			{
				note_id: '00000000-0000-4000-8000-000000000103',
				obligation_id: 'search-wording',
				title: 'Consider whether the search description could be clearer',
				body: 'An optional language refinement remains for later product review.',
				data: { expected: 'Search descriptions use familiar settings language.' },
				tasks: ['T1'],
				criteria: ['C1'],
				outcome: 'pending',
				required: false,
				issue: null,
				at: '2026-10-04T14:15:00Z',
				session_uuid: '00000000-0000-4000-8000-000000000203'
			}
		]
	};
}
