import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { InspectClient } from '../src/inspectClient.js';
import { buildToolServer, buildTools } from '../src/mcp/tools.js';
import { SETTINGS_DEFAULTS } from '../src/settings.js';
import type { PromptAssistantSettings, SseEvent } from '../src/types.js';
import { stubFetch } from './fakes.js';

function settings(overrides: Partial<PromptAssistantSettings> = {}): PromptAssistantSettings {
	return { ...SETTINGS_DEFAULTS, enabled: true, ...overrides };
}

function noopClient(): InspectClient {
	return new InspectClient('http://inspect.local', stubFetch({}));
}

// A ToolContext is built per-turn and always needs a claudeHome, even when the
// test does not exercise read_memory — an unused fixture path is fine there.
const UNUSED_CLAUDE_HOME = '/tmp/prompt-assistant-tests-unused-claude-home';

describe('tool gating by read_* settings', () => {
	it('registers all six read tools plus propose_edit when every read_* is on', () => {
		const tools = buildTools({
			inspectClient: noopClient(),
			settings: settings({ read_memory: true }),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		expect(tools.map((t) => t.name).sort()).toEqual(
			[
				'compare_across_projects',
				'dry_run',
				'get_prompt',
				'list_edits',
				'list_projects',
				'list_prompt_versions',
				'propose_edit',
				'read_memory',
				'read_project_instructions'
			].sort()
		);
	});

	it('drops read_memory when read_memory is off (the spec default)', () => {
		const tools = buildTools({
			inspectClient: noopClient(),
			settings: settings(),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		expect(tools.map((t) => t.name)).not.toContain('read_memory');
	});

	it('registers read_memory when read_memory is on, independently of the other read_* settings', () => {
		const tools = buildTools({
			inspectClient: noopClient(),
			settings: settings({
				read_memory: true,
				read_prompt: false,
				read_edits: false,
				read_project_instructions: false,
				read_versions: false,
				read_other_projects: false
			}),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		expect(tools.map((t) => t.name).sort()).toEqual(['propose_edit', 'read_memory'].sort());
	});

	it('drops get_prompt (and dry_run, which needs it) when read_prompt is off', () => {
		const tools = buildTools({
			inspectClient: noopClient(),
			settings: settings({ read_prompt: false }),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const names = tools.map((t) => t.name);
		expect(names).not.toContain('get_prompt');
		expect(names).not.toContain('dry_run');
		expect(names).toContain('list_edits');
	});

	it('drops list_edits (and dry_run) when read_edits is off', () => {
		const tools = buildTools({
			inspectClient: noopClient(),
			settings: settings({ read_edits: false }),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const names = tools.map((t) => t.name);
		expect(names).not.toContain('list_edits');
		expect(names).not.toContain('dry_run');
		expect(names).toContain('get_prompt');
	});

	it('drops read_project_instructions and list_prompt_versions independently', () => {
		const tools = buildTools({
			inspectClient: noopClient(),
			settings: settings({ read_project_instructions: false, read_versions: false }),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const names = tools.map((t) => t.name);
		expect(names).not.toContain('read_project_instructions');
		expect(names).not.toContain('list_prompt_versions');
	});

	it('always registers propose_edit, even with every read_* off', () => {
		const tools = buildTools({
			inspectClient: noopClient(),
			settings: settings({
				read_prompt: false,
				read_edits: false,
				read_project_instructions: false,
				read_versions: false,
				read_other_projects: false
			}),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		expect(tools.map((t) => t.name)).toEqual(['propose_edit']);
	});

	it('exposes every registered tool as an allowed mcp__prompt_assistant__<name> wire name', () => {
		const { allowedTools } = buildToolServer({
			inspectClient: noopClient(),
			settings: settings({ read_project_instructions: false, read_memory: true }),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		expect(allowedTools).toContain('mcp__prompt_assistant__get_prompt');
		expect(allowedTools).toContain('mcp__prompt_assistant__propose_edit');
		expect(allowedTools).toContain('mcp__prompt_assistant__read_memory');
		expect(allowedTools).not.toContain('mcp__prompt_assistant__read_project_instructions');
	});
});

describe('read_memory tool', () => {
	let claudeHome: string;

	beforeEach(async () => {
		claudeHome = await fs.mkdtemp(path.join(os.tmpdir(), 'prompt-assistant-memory-'));
	});
	afterEach(async () => {
		await fs.rm(claudeHome, { recursive: true, force: true });
	});

	it("reads the project's memory files through the request's project path", async () => {
		const projectPath = '/mnt/data/projects/temet/temet-proxy';
		const slug = projectPath.split('/').join('-');
		const memoryDir = path.join(claudeHome, 'projects', slug, 'memory');
		await fs.mkdir(memoryDir, { recursive: true });
		await fs.writeFile(path.join(memoryDir, 'MEMORY.md'), '- an index entry\n');
		await fs.writeFile(path.join(memoryDir, 'a-lesson.md'), '# A lesson\ndetails');

		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/421514': {
					id: 421514,
					session_id: 's',
					project: projectPath,
					human_prompt: '',
					last_user_message: '',
					model: null
				}
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings({ read_memory: true }),
			requestId: 421514,
			emit: () => {},
			issuedProposals: [],
			claudeHome
		});
		const readMemory = tools.find((t) => t.name === 'read_memory');
		if (!readMemory) throw new Error('read_memory not registered');
		const result = await readMemory.handler({ request_id: 421514 }, undefined);
		const payload = JSON.parse((result.content[0] as { text: string }).text) as {
			project: string;
			files: Array<{ path: string; content: string }>;
		};
		expect(payload.project).toBe(projectPath);
		expect(payload.files.map((f) => f.path)).toEqual(['MEMORY.md', 'a-lesson.md']);
		expect(payload.files[0]?.content).toBe('- an index entry\n');
	});

	it('returns an empty file list, not an error, when the project has no memory directory yet', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/1': {
					id: 1,
					session_id: 's',
					project: '/mnt/data/projects/never-opened',
					human_prompt: '',
					last_user_message: '',
					model: null
				}
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings({ read_memory: true }),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome
		});
		const readMemory = tools.find((t) => t.name === 'read_memory');
		if (!readMemory) throw new Error('read_memory not registered');
		const result = await readMemory.handler({ request_id: 1 }, undefined);
		const payload = JSON.parse((result.content[0] as { text: string }).text) as { files: unknown[] };
		expect(payload.files).toEqual([]);
	});
});

describe('propose_edit: apply vs suggest', () => {
	function findProposeEdit(tools: ReturnType<typeof buildTools>) {
		const t = tools.find((tool) => tool.name === 'propose_edit');
		if (!t) throw new Error('propose_edit not registered');
		return t;
	}

	it('suggest mode: records and streams the proposal but never calls the filter-rule API', async () => {
		let createCalled = false;
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/421514': {
					id: 421514,
					session_id: 's',
					project: null,
					human_prompt: '',
					last_user_message: 'Prefer the dedicated file/search tools over shell commands.',
					model: null,
					system_blocks: { prompt_sha256: null, composition_id: null, composition_version: null, blocks: [] }
				},
				'POST /api/filter/rules': () => {
					createCalled = true;
					return { rule: { id: '99' }, warnings: [] };
				}
			})
		);
		const events: SseEvent[] = [];
		const issuedProposals: SseEvent[] = [];
		const tools = buildTools({
			inspectClient: client,
			settings: settings({ may_apply: 'suggest' }),
			requestId: 421514,
			emit: (e) => events.push(e),
			issuedProposals: issuedProposals as never,
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const tool = findProposeEdit(tools);
		await tool.handler(
			{
				title: 'Reword the tool-search sentence',
				edit_id: null,
				action: 'replace',
				match: 'Prefer the dedicated file/search tools over shell commands.',
				text: 'Prefer the dedicated file/search tools.',
				reach: 'Every model, current CC'
			},
			undefined
		);
		expect(createCalled).toBe(false);
		expect(events.map((e) => e.event)).toEqual(['proposal']);
		const proposalEvent = events[0] as Extract<SseEvent, { event: 'proposal' }>;
		expect(proposalEvent.data.before).toBe('Prefer the dedicated file/search tools over shell commands.');
		expect(proposalEvent.data.after).toBe('Prefer the dedicated file/search tools.');
		expect(proposalEvent.data.rule_text).toBe('Prefer the dedicated file/search tools.');
	});

	it('append: rule_text is the raw addition only, distinct from the match-concatenated after', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/421514': {
					id: 421514,
					session_id: 's',
					project: null,
					human_prompt: '',
					last_user_message: 'Existing paragraph.',
					model: null,
					system_blocks: { prompt_sha256: null, composition_id: null, composition_version: null, blocks: [] }
				}
			})
		);
		const events: SseEvent[] = [];
		const tools = buildTools({
			inspectClient: client,
			settings: settings({ may_apply: 'suggest' }),
			requestId: 421514,
			emit: (e) => events.push(e),
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		await findProposeEdit(tools).handler(
			{
				title: 'Add a follow-up sentence',
				edit_id: null,
				action: 'append',
				match: 'Existing paragraph.',
				text: ' One more sentence.',
				reach: 'All models'
			},
			undefined
		);
		const proposalEvent = events[0] as Extract<SseEvent, { event: 'proposal' }>;
		expect(proposalEvent.data.rule_text).toBe(' One more sentence.');
		expect(proposalEvent.data.after).toBe('Existing paragraph. One more sentence.');
		expect(proposalEvent.data.rule_text).not.toContain('Existing paragraph.');
	});

	it('drop: rule_text is null', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/1': {
					id: 1,
					session_id: 's',
					project: null,
					human_prompt: '',
					last_user_message: 'Some captured text with a phrase in it.',
					model: null
				}
			})
		);
		const events: SseEvent[] = [];
		const tools = buildTools({
			inspectClient: client,
			settings: settings({ may_apply: 'suggest' }),
			requestId: 1,
			emit: (e) => events.push(e),
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		await findProposeEdit(tools).handler(
			{
				title: 'Trim the phrase',
				edit_id: null,
				action: 'drop',
				match: 'a phrase in it',
				text: '',
				reach: 'All models'
			},
			undefined
		);
		const proposalEvent = events[0] as Extract<SseEvent, { event: 'proposal' }>;
		expect(proposalEvent.data.rule_text).toBeNull();
	});

	it('apply mode: creates the filter rule immediately and emits applied', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/421514': {
					id: 421514,
					session_id: 's',
					project: null,
					human_prompt: '',
					last_user_message: 'Some captured text with a phrase in it.',
					model: null
				},
				'POST /api/filter/rules': { rule: { id: '101' }, warnings: [] }
			})
		);
		const events: SseEvent[] = [];
		const tools = buildTools({
			inspectClient: client,
			settings: settings({ may_apply: 'apply' }),
			requestId: 421514,
			emit: (e) => events.push(e),
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const tool = findProposeEdit(tools);
		await tool.handler(
			{
				title: 'Trim the phrase',
				edit_id: null,
				action: 'drop',
				match: 'a phrase in it',
				text: '',
				reach: 'All models'
			},
			undefined
		);
		expect(events.map((e) => e.event)).toEqual(['proposal', 'applied']);
		const applied = events[1] as Extract<SseEvent, { event: 'applied' }>;
		expect(applied.data.rule_id).toBe('101');
	});

	it('before is empty when the claimed match text is not actually in the captured prompt', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/1': {
					id: 1,
					session_id: 's',
					project: null,
					human_prompt: '',
					last_user_message: 'Only this text was really captured.',
					model: null
				}
			})
		);
		const events: SseEvent[] = [];
		const tools = buildTools({
			inspectClient: client,
			settings: settings({ may_apply: 'suggest' }),
			requestId: 1,
			emit: (e) => events.push(e),
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		await findProposeEdit(tools).handler(
			{
				title: 'Made up',
				edit_id: null,
				action: 'replace',
				match: 'text the model invented',
				text: 'replacement',
				reach: 'n/a'
			},
			undefined
		);
		const proposalEvent = events[0] as Extract<SseEvent, { event: 'proposal' }>;
		expect(proposalEvent.data.before).toBe('');
	});

	it('apply mode: forwards project_path to the filter-rule create payload for a project-scoped proposal', async () => {
		let sentBody: Record<string, unknown> | undefined;
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/1': {
					id: 1,
					session_id: 's',
					project: null,
					human_prompt: '',
					last_user_message: 'text',
					model: null
				},
				'POST /api/filter/rules': (init) => {
					sentBody = JSON.parse((init?.body as string) ?? '{}');
					return { rule: { id: '55' }, warnings: [] };
				}
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings({ may_apply: 'apply' }),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		await findProposeEdit(tools).handler(
			{
				title: 'Project-only tweak',
				edit_id: null,
				action: 'replace',
				match: 'text',
				text: 'new text',
				reach: 'n/a',
				project_path: '/mnt/data/projects/other'
			},
			undefined
		);
		expect(sentBody?.project_path).toBe('/mnt/data/projects/other');
	});

	it('suggest mode: streams project_path on the proposal event when set', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/1': {
					id: 1,
					session_id: 's',
					project: null,
					human_prompt: '',
					last_user_message: 'text',
					model: null
				}
			})
		);
		const events: SseEvent[] = [];
		const tools = buildTools({
			inspectClient: client,
			settings: settings({ may_apply: 'suggest' }),
			requestId: 1,
			emit: (e) => events.push(e),
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		await findProposeEdit(tools).handler(
			{
				title: 'Project-only tweak',
				edit_id: null,
				action: 'replace',
				match: 'text',
				text: 'new text',
				reach: 'n/a',
				project_path: '/mnt/data/projects/other'
			},
			undefined
		);
		const proposalEvent = events[0] as Extract<SseEvent, { event: 'proposal' }>;
		expect(proposalEvent.data.project_path).toBe('/mnt/data/projects/other');
	});
});

describe('read_other_projects gating', () => {
	it('registers list_projects and compare_across_projects when on (the spec default)', () => {
		const tools = buildTools({
			inspectClient: noopClient(),
			settings: settings(),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const names = tools.map((t) => t.name);
		expect(names).toContain('list_projects');
		expect(names).toContain('compare_across_projects');
	});

	it('drops both cross-project tools when read_other_projects is off', () => {
		const tools = buildTools({
			inspectClient: noopClient(),
			settings: settings({ read_other_projects: false }),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const names = tools.map((t) => t.name);
		expect(names).not.toContain('list_projects');
		expect(names).not.toContain('compare_across_projects');
	});
});

describe('list_projects tool', () => {
	it('maps the /api/projects rows to path/name/counts', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/projects': [
					{
						project_id: 1,
						project_uuid: 'u1',
						project_path: '/mnt/data/projects/a',
						custom_name: 'Project A',
						session_count: 4,
						request_count: 40,
						first_seen: '2026-09-01T00:00:00Z',
						last_seen: '2026-09-20T00:00:00Z'
					}
				]
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings(),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const listProjects = tools.find((t) => t.name === 'list_projects');
		if (!listProjects) throw new Error('list_projects not registered');
		const result = await listProjects.handler({}, undefined);
		const payload = JSON.parse((result.content[0] as { text: string }).text) as Array<{
			project_path: string;
			name: string | null;
			session_count: number;
		}>;
		expect(payload).toEqual([
			{
				project_path: '/mnt/data/projects/a',
				name: 'Project A',
				session_count: 4,
				request_count: 40,
				last_seen: '2026-09-20T00:00:00Z'
			}
		]);
	});
});

describe('compare_across_projects tool', () => {
	function baseRoutes(currentProject: string | null) {
		return {
			'GET /api/requests/1': {
				id: 1,
				session_id: 's',
				project: currentProject,
				human_prompt: '',
				last_user_message: '',
				model: null
			}
		};
	}

	it('excludes the current project and checks the remaining ones for the match text', async () => {
		const now = new Date('2026-09-23T00:00:00Z');
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				...baseRoutes('/mnt/data/projects/current'),
				'GET /api/projects': [
					{
						project_id: 1,
						project_uuid: 'u1',
						project_path: '/mnt/data/projects/current',
						custom_name: null,
						session_count: 1,
						request_count: 1,
						first_seen: '2026-09-01T00:00:00Z',
						last_seen: now.toISOString()
					},
					{
						project_id: 2,
						project_uuid: 'u2',
						project_path: '/mnt/data/projects/other',
						custom_name: 'Other',
						session_count: 2,
						request_count: 5,
						first_seen: '2026-09-01T00:00:00Z',
						last_seen: now.toISOString()
					}
				],
				'GET /api/content/search': {
					sessions: [
						{
							session_id: 's-other',
							project_path: '/mnt/data/projects/other',
							matches: [{ request_id: 900, before: 'before ', matched: 'no comments', after: ' after' }]
						}
					]
				}
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings(),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const compare = tools.find((t) => t.name === 'compare_across_projects');
		if (!compare) throw new Error('compare_across_projects not registered');
		const result = await compare.handler({ match: 'no comments' }, undefined);
		const payload = JSON.parse((result.content[0] as { text: string }).text) as {
			current_project: string | null;
			projects: Array<{ project_path: string; match_in_prompt: boolean; instruction_hits: number }>;
		};
		expect(payload.current_project).toBe('/mnt/data/projects/current');
		expect(payload.projects).toHaveLength(1);
		expect(payload.projects[0]?.project_path).toBe('/mnt/data/projects/other');
		expect(payload.projects[0]?.match_in_prompt).toBe(true);
	});

	it('never includes file excerpts for a project with no instruction hits', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				...baseRoutes('/mnt/data/projects/current'),
				'GET /api/projects': [
					{
						project_id: 2,
						project_uuid: 'u2',
						project_path: '/mnt/data/projects/other',
						custom_name: 'Other',
						session_count: 2,
						request_count: 5,
						first_seen: '2026-09-01T00:00:00Z',
						last_seen: new Date().toISOString()
					}
				],
				'GET /api/content/search': { sessions: [] }
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings(),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const compare = tools.find((t) => t.name === 'compare_across_projects');
		if (!compare) throw new Error('compare_across_projects not registered');
		const result = await compare.handler({ match: 'nothing to see here' }, undefined);
		const payload = JSON.parse((result.content[0] as { text: string }).text) as {
			projects: Array<{ instruction_hits: number; verdict_hint: string; excerpts?: unknown[] }>;
		};
		expect(payload.projects[0]?.instruction_hits).toBe(0);
		expect(payload.projects[0]?.verdict_hint).toBe('none');
		expect(payload.projects[0]?.excerpts).toBeUndefined();
	});

	it('drills into named projects with project_paths, returning full instructions and more match excerpts', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				...baseRoutes('/mnt/data/projects/current'),
				'GET /api/projects': [
					{
						project_id: 2,
						project_uuid: 'u2',
						project_path: '/mnt/data/projects/other',
						custom_name: 'Other',
						session_count: 2,
						request_count: 5,
						first_seen: '2026-09-01T00:00:00Z',
						last_seen: new Date().toISOString()
					}
				],
				'GET /api/content/search': {
					sessions: [
						{
							session_id: 's-other',
							project_path: '/mnt/data/projects/other',
							matches: [{ request_id: 900, before: 'before ', matched: 'no comments', after: ' after' }]
						}
					]
				}
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings(),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const compare = tools.find((t) => t.name === 'compare_across_projects');
		if (!compare) throw new Error('compare_across_projects not registered');
		const result = await compare.handler(
			{ match: 'no comments', project_paths: ['/mnt/data/projects/other', '/mnt/data/projects/unknown'] },
			undefined
		);
		const payload = JSON.parse((result.content[0] as { text: string }).text) as {
			mode: string;
			unknown_project_paths: string[];
			projects: Array<{ project_path: string; match_found: boolean; instructions: unknown[] }>;
		};
		expect(payload.mode).toBe('drill_down');
		expect(payload.unknown_project_paths).toEqual(['/mnt/data/projects/unknown']);
		expect(payload.projects).toHaveLength(1);
		expect(payload.projects[0]?.project_path).toBe('/mnt/data/projects/other');
		expect(payload.projects[0]?.match_found).toBe(true);
		expect(payload.projects[0]).toHaveProperty('instructions');
	});

	it('excludes a project not seen within the requested day window', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				...baseRoutes(null),
				'GET /api/projects': [
					{
						project_id: 2,
						project_uuid: 'u2',
						project_path: '/mnt/data/projects/stale',
						custom_name: null,
						session_count: 1,
						request_count: 1,
						first_seen: '2020-01-01T00:00:00Z',
						last_seen: '2020-01-02T00:00:00Z'
					}
				],
				'GET /api/content/search': { sessions: [] }
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings(),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const compare = tools.find((t) => t.name === 'compare_across_projects');
		if (!compare) throw new Error('compare_across_projects not registered');
		const result = await compare.handler({ match: 'anything', since_days: 7 }, undefined);
		const payload = JSON.parse((result.content[0] as { text: string }).text) as { projects: unknown[] };
		expect(payload.projects).toEqual([]);
	});
});

describe('read_project_instructions: generalized to any known project', () => {
	it('still works with request_id (unchanged default form)', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/requests/1': {
					id: 1,
					session_id: 's',
					project: null,
					human_prompt: '',
					last_user_message: '',
					model: null
				}
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings(),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const t = tools.find((tool) => tool.name === 'read_project_instructions');
		if (!t) throw new Error('read_project_instructions not registered');
		const result = await t.handler({ request_id: 1 }, undefined);
		const payload = JSON.parse((result.content[0] as { text: string }).text) as { project: string | null };
		expect(payload.project).toBeNull();
	});

	it('rejects a project_path that is not in list_projects', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/projects': []
			})
		);
		const tools = buildTools({
			inspectClient: client,
			settings: settings(),
			requestId: 1,
			emit: () => {},
			issuedProposals: [],
			claudeHome: UNUSED_CLAUDE_HOME
		});
		const t = tools.find((tool) => tool.name === 'read_project_instructions');
		if (!t) throw new Error('read_project_instructions not registered');
		const result = await t.handler({ project_path: '/not/a/known/project' }, undefined);
		expect(result.isError).toBe(true);
	});
});
