import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '$lib/api';
import type { PromptClassDetailDto, PromptClassDto } from '$lib/types';
import {
	findLatestCaptureInProject,
	type ProjectPromptLookupFetchers
} from './projectPromptLookup';

function detail(overrides: Partial<PromptClassDetailDto> = {}): PromptClassDetailDto {
	return {
		lane: 'main',
		subagent_persona: null,
		observation_count: 1,
		first_seen: '2026-09-01T00:00:00Z',
		last_seen: '2026-09-20T00:00:00Z',
		model_breakdown: [],
		sample_blocks: null,
		registration: {
			registered: false,
			composition_id: null,
			label: null,
			mode: null,
			agent_definition_path: null
		},
		recent_observations: [],
		observations_total_count: 0,
		observations_offset: 0,
		observations_limit: 20,
		...overrides
	};
}

function classDto(overrides: Partial<PromptClassDto> = {}): PromptClassDto {
	return {
		lane: 'main',
		subagent_persona: null,
		observation_count: 3,
		first_seen: '2026-09-01T00:00:00Z',
		last_seen: '2026-09-20T00:00:00Z',
		block_count: 4,
		registration: {
			registered: false,
			composition_id: null,
			label: null,
			mode: null,
			agent_definition_path: null
		},
		model_breakdown: [],
		...overrides
	};
}

function fetchers(
	overrides: Partial<ProjectPromptLookupFetchers> = {}
): ProjectPromptLookupFetchers {
	return {
		loadMainDetail: vi.fn(),
		loadSubagentDetail: vi.fn(),
		loadClasses: vi.fn(),
		...overrides
	} as ProjectPromptLookupFetchers;
}

describe('findLatestCaptureInProject', () => {
	it('resolves the latest observation as the found request id (main lane)', async () => {
		const loadMainDetail = vi.fn().mockResolvedValue(
			detail({
				recent_observations: [
					{
						request_id: '99001',
						session_id: 's1',
						created_at: 'x',
						content_sha256: 'a',
						altered: false,
						model: 'claude-sonnet-5'
					}
				]
			})
		);
		const result = await findLatestCaptureInProject(
			{ lane: 'main', persona: null },
			'claude-sonnet-5',
			'/mnt/data/projects/other',
			fetchers({ loadMainDetail })
		);
		expect(result).toEqual({ kind: 'found', requestId: 99001 });
		expect(loadMainDetail).toHaveBeenCalledWith({
			model: 'claude-sonnet-5',
			project_path: '/mnt/data/projects/other',
			limit: 1
		});
	});

	it('routes a subagent class through loadSubagentDetail with the persona', async () => {
		const loadSubagentDetail = vi.fn().mockResolvedValue(
			detail({
				lane: 'subagent',
				subagent_persona: 'code-reviewer',
				recent_observations: [
					{
						request_id: '77',
						session_id: 's2',
						created_at: 'x',
						content_sha256: 'b',
						altered: false,
						model: null
					}
				]
			})
		);
		const result = await findLatestCaptureInProject(
			{ lane: 'subagent', persona: 'code-reviewer' },
			null,
			'/mnt/data/projects/other',
			fetchers({ loadSubagentDetail })
		);
		expect(result).toEqual({ kind: 'found', requestId: 77 });
		expect(loadSubagentDetail).toHaveBeenCalledWith('code-reviewer', {
			model: undefined,
			project_path: '/mnt/data/projects/other',
			limit: 1
		});
	});

	it('falls back to the project class listing when there are no observations', async () => {
		const loadMainDetail = vi.fn().mockResolvedValue(detail({ recent_observations: [] }));
		const loadClasses = vi.fn().mockResolvedValue([classDto()]);
		const result = await findLatestCaptureInProject(
			{ lane: 'main', persona: null },
			'claude-sonnet-5',
			'/mnt/data/projects/other',
			fetchers({ loadMainDetail, loadClasses })
		);
		expect(result.kind).toBe('no_capture');
		expect(loadClasses).toHaveBeenCalledWith({ project_path: '/mnt/data/projects/other' });
		if (result.kind === 'no_capture') expect(result.availableClasses).toHaveLength(1);
	});

	it('falls back to the project class listing on a 404 (class never observed in this project)', async () => {
		const loadMainDetail = vi.fn().mockRejectedValue(new ApiError('not found', 404, '/x'));
		const loadClasses = vi.fn().mockResolvedValue([]);
		const result = await findLatestCaptureInProject(
			{ lane: 'main', persona: null },
			'claude-sonnet-5',
			'/mnt/data/projects/other',
			fetchers({ loadMainDetail, loadClasses })
		);
		expect(result).toEqual({ kind: 'no_capture', availableClasses: [] });
	});

	it('reports a non-404 error as an error, not a silent no_capture', async () => {
		const loadMainDetail = vi.fn().mockRejectedValue(new ApiError('server exploded', 500, '/x'));
		const result = await findLatestCaptureInProject(
			{ lane: 'main', persona: null },
			'claude-sonnet-5',
			'/mnt/data/projects/other',
			fetchers({ loadMainDetail })
		);
		expect(result).toEqual({ kind: 'error', message: 'server exploded' });
	});
});
