import { z } from 'zod';
import type { SdkMcpToolDefinition } from '@anthropic-ai/claude-agent-sdk';
import { tool } from '@anthropic-ai/claude-agent-sdk';
import type { ToolContext } from './tools.js';
import { readProjectInstructions } from './projectFiles.js';
import type { CrossProjectFinding, ProjectSummary, ProposalAction } from '../types.js';
import {
	assembleCompareResponse,
	extractKeyTerms,
	paragraphsWithHits,
	type RawProjectAnalysis
} from './crossProjectAnalysis.js';

const DEFAULT_SINCE_DAYS = 30;
const DRILL_DOWN_MATCHES_PER_PROJECT = 10;

function textResult(payload: unknown): { content: Array<{ type: 'text'; text: string }> } {
	return { content: [{ type: 'text', text: JSON.stringify(payload) }] };
}
function errorResult(message: string): { content: Array<{ type: 'text'; text: string }>; isError: true } {
	return { content: [{ type: 'text', text: message }], isError: true };
}

export function buildListProjectsTool(ctx: ToolContext): SdkMcpToolDefinition<any> {
	return tool(
		'list_projects',
		'List every project mike has captured traffic for on this machine: path, name, and session/request counts. The system prompt these projects run is shared, so this is how the assistant finds who else a global edit would reach.',
		{},
		async () => {
			ctx.emit({ event: 'status', data: { phase: 'listing projects' } });
			try {
				const projects = await ctx.inspectClient.listProjects();
				return textResult(
					projects.map((p) => ({
						project_path: p.project_path,
						name: p.custom_name,
						session_count: p.session_count,
						request_count: p.request_count,
						last_seen: p.last_seen
					}))
				);
			} catch (err) {
				return errorResult(`could not list projects: ${(err as Error).message}`);
			}
		}
	);
}

function withinDays(lastSeen: string, days: number, now: Date): boolean {
	const cutoff = now.getTime() - days * 24 * 60 * 60 * 1000;
	const seen = Date.parse(lastSeen);
	return Number.isFinite(seen) && seen >= cutoff;
}

async function resolveCurrentProjectPath(ctx: ToolContext): Promise<string | null> {
	try {
		const request = await ctx.inspectClient.getRequest(ctx.requestId);
		return request.project;
	} catch {
		return null;
	}
}

function selectCandidateProjects(
	projects: ProjectSummary[],
	currentProjectPath: string | null,
	sinceDays: number,
	now: Date
): ProjectSummary[] {
	return projects
		.filter((p) => p.project_path !== currentProjectPath)
		.filter((p) => withinDays(p.last_seen, sinceDays, now));
}

async function gatherRawAnalysis(
	ctx: ToolContext,
	project: ProjectSummary,
	matchText: string,
	keyTerms: string[]
): Promise<RawProjectAnalysis> {
	const [matches, files] = await Promise.all([
		ctx.inspectClient.searchContentInProject(matchText, project.project_path, 1),
		readProjectInstructions(project.project_path)
	]);
	const hitParagraphs = files.flatMap((f) =>
		paragraphsWithHits(f.content, keyTerms).map((text) => ({ path: f.path, text }))
	);
	return {
		project_path: project.project_path,
		project_name: project.custom_name,
		recent_sessions: project.session_count,
		match_in_prompt: matches.length > 0,
		hitParagraphs
	};
}

async function buildFullFinding(ctx: ToolContext, project: ProjectSummary, matchText: string): Promise<CrossProjectFinding> {
	const [matchExcerpts, instructions] = await Promise.all([
		ctx.inspectClient.searchContentInProject(matchText, project.project_path, DRILL_DOWN_MATCHES_PER_PROJECT),
		readProjectInstructions(project.project_path)
	]);
	return {
		project_path: project.project_path,
		project_name: project.custom_name,
		last_seen: project.last_seen,
		match_found: matchExcerpts.length > 0,
		match_excerpts: matchExcerpts,
		instructions
	};
}

async function runDrillDown(ctx: ToolContext, match: string, projectPaths: string[]) {
	const known = await ctx.inspectClient.listProjects();
	const byPath = new Map(known.map((p) => [p.project_path, p]));
	const unknownPaths = projectPaths.filter((p) => !byPath.has(p));
	const targets = projectPaths.map((p) => byPath.get(p)).filter((p): p is ProjectSummary => p !== undefined);
	const projects = await Promise.all(targets.map((p) => buildFullFinding(ctx, p, match)));
	return textResult({ mode: 'drill_down' as const, unknown_project_paths: unknownPaths, projects });
}

function buildCompareSchema() {
	return {
		match: z.string(),
		action: z.enum(['replace', 'append', 'drop']).optional(),
		text: z.string().optional(),
		edit_id: z.string().nullable().optional(),
		since_days: z
			.number()
			.int()
			.positive()
			.optional()
			.describe('Only consider projects with traffic in the last N days (default 30).'),
		project_paths: z
			.array(z.string())
			.optional()
			.describe(
				'Drill into these specific projects (from list_projects) instead of scanning every recent one: returns full instruction files and up to 10 match excerpts per project, no size cap. Use this after the default compact scan flags a project worth reading in full.'
			)
	};
}

export function buildCompareAcrossProjectsTool(ctx: ToolContext): SdkMcpToolDefinition<any> {
	return tool(
		'compare_across_projects',
		"Default (no project_paths): a compact, size-bounded row per recently-active project (name, path, recent session count, whether the edit's match text occurs in its captured prompts, a count of CLAUDE.md/rules paragraphs mentioning the edit's key terms, and a cheap verdict_hint — none/possible_conflict/agrees, a heuristic the assistant should verify, not trust blindly). Full excerpt text is included only for the handful of projects with the most hits. Pass project_paths to drill into specific projects afterward with full instruction files and more match excerpts — do that instead of re-running the default scan to read more.",
		buildCompareSchema(),
		async ({ match, action, since_days, project_paths }) => {
			ctx.emit({ event: 'status', data: { phase: 'comparing across projects' } });
			try {
				if (project_paths && project_paths.length > 0) {
					return await runDrillDown(ctx, match, project_paths);
				}
				const effectiveSinceDays = since_days ?? DEFAULT_SINCE_DAYS;
				const keyTerms = extractKeyTerms(match);
				const [currentProjectPath, projects] = await Promise.all([
					resolveCurrentProjectPath(ctx),
					ctx.inspectClient.listProjects()
				]);
				const candidates = selectCandidateProjects(projects, currentProjectPath, effectiveSinceDays, new Date());
				const analyses = await Promise.all(
					candidates.map((p) => gatherRawAnalysis(ctx, p, match, keyTerms))
				);
				const result = assembleCompareResponse(
					analyses,
					currentProjectPath,
					effectiveSinceDays,
					action as ProposalAction | undefined
				);
				return textResult(result);
			} catch (err) {
				return errorResult(`could not compare across projects: ${(err as Error).message}`);
			}
		}
	);
}
