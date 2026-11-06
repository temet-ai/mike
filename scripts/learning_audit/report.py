import collections
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import urllib.request
import urllib.parse
from . import delivery
from .queries import MIGRATION_DEPENDENT, queries

ROOT = Path(__file__).resolve().parents[2]


def publication_status(path=None):
    path = Path(path) if path else Path.home() / '.local/state/mike-learning-promotion/latest.json'
    if not path.exists():
        return {'state': 'missing'}
    value = json.loads(path.read_text())
    return {'state': 'conflict' if value['conflicts'] else 'published',
            'updated_at': dt.datetime.fromtimestamp(path.stat().st_mtime, dt.timezone.utc).isoformat(),
            'lessons': value['lessons'], 'memory_projects': value['memory_projects'],
            'skill_installations': len(value['skills']), 'conflicts': value['conflicts']}


def sync_reports(output):
    """Publish only audit documents, never another project's generated memory."""
    from knowledge.collector import Client, scan_root
    from knowledge.machine import machine_identity
    config = json.loads((ROOT / '.mike/knowledge.json').read_text())
    directory = Path(output).resolve(strict=True)
    documents = scan_root(directory, [])
    client = Client('http://127.0.0.1:9989')
    machine = machine_identity()
    location = client.request('/api/knowledge/locations', {
        'project': config['project_uuid'], **machine, 'root_path': str(directory)})
    source = client.request('/api/knowledge/sources', {
        'location_uuid': location['location_uuid'], 'name': 'weekly-learning-audit',
        'root_path': str(directory)})
    return client.request(f"/api/knowledge/sources/{source['source_uuid']}/scan", {
        'generation': source['generation'], 'documents': documents})


def query(sql):
    command = ('set -a; test ! -f .env || source .env >/dev/null 2>&1; set +a; '
               'export PGOPTIONS="-c default_transaction_read_only=on -c statement_timeout=120000 -c lock_timeout=2000"; '
               'psql "$MIKE_DATABASE_URL" -X -qAt -v ON_ERROR_STOP=1')
    result = subprocess.run(['bash', '-c', command], cwd=ROOT,
                            input='SELECT row_to_json(a) FROM (' + sql + ') a;',
                            text=True, capture_output=True, timeout=150)
    if result.returncode:
        raise RuntimeError('Database audit query failed: ' + result.stderr)
    return [json.loads(line) for line in result.stdout.splitlines() if line]


def skill_ledger():
    path = Path.home() / '.local/state/mike-learning-promotion/skill-installs.json'
    return json.loads(path.read_text()) if path.exists() else {}


def skill_manifest_names():
    """Names of the skills whose SKILL.md the promoter wrote and still owns."""
    path = Path.home() / '.local/state/mike-learning-promotion/manifest.json'
    manifest = json.loads(path.read_text()) if path.exists() else {}
    return {Path(file).parent.name for file in manifest if Path(file).name == 'SKILL.md'}


def generated_skill_loads(data):
    """A load is mike-made when the promoter's ledger or manifest records the skill, whatever it is named."""
    ledger = skill_ledger()
    manifest = skill_manifest_names()
    ledger_names = {key.split(':', 1)[1] for key in ledger}
    return [{**row, 'generated': bool(row['skill'])
             and (f"{row.get('project_uuid')}:{row['skill']}" in ledger or row['skill'] in manifest
                  or (not row.get('project_uuid') and row['skill'] in ledger_names))}
            for row in data.get('skill_loads', [])]


def memory_grammar_totals(data):
    recall = data.get('memory_recall_tags', [])
    update = data.get('memory_update_tags', [])
    return {'recall_tag_requests': sum(r['requests_with_recall_tags'] for r in recall),
            'recall_injected': sum(r['injected'] + r['delivered_out_of_band'] for r in recall),
            'update_tags_found': sum(r['tags_found'] for r in update),
            'update_lessons_written': sum(r['lessons_written'] for r in update),
            'update_lessons_skipped': sum(r['lessons_skipped'] for r in update),
            'harvested_lessons': sum(r['lessons'] for r in data.get('memory_harvested_lessons', []))}


def findings(data):
    out = []
    if 'memory_update_tags' in data:
        totals = memory_grammar_totals(data)
        if not totals['recall_tag_requests'] and not totals['update_tags_found']:
            out.append({'kind': 'memory_grammar_unused', 'project': 'global', 'evidence': totals})
    for row in data['embedding_setting']:
        if row['value'] is False:
            out.append({'kind': 'embedding_disabled', 'project': 'global', 'evidence': row})
    injected = {r['project'] for r in data['injections'] if r['injections']}
    for row in data['turns']:
        if row.get('genuine_turns', row['turns']) and row['project'] not in injected:
            out.append({'kind': 'no_recorded_lesson_delivery', 'project': row['project'], 'evidence': row})
        if row.get('missing_provenance') or row.get('stale_outcome_labels'):
            out.append({'kind': 'outcome_provenance_incomplete', 'project': row['project'], 'evidence': row})
    for row in data['lessons']:
        if row['source'] == 'operator' and row['pending_embeddings']:
            out.append({'kind': 'operator_lessons_unindexed', 'project': row['project'], 'evidence': row})
    if not any(r['family'] == 'action_effect' and r['rows'] for r in data['effects']):
        out.append({'kind': 'effect_measurement_missing', 'project': 'global'})
    skill_loads = generated_skill_loads(data)
    if skill_loads and not any(r['generated'] for r in skill_loads):
        out.append({'kind': 'generated_skills_never_loaded', 'project': 'global',
                    'evidence': {'total_loads': sum(r['loads'] for r in skill_loads),
                                 'generated_loads': sum(r['loads'] for r in skill_loads if r['generated'])}})
    out += llm_findings(data)
    out += live_findings(data)
    return out


def rate_text(part, whole):
    return f'{part} of {whole} ({part * 100 / whole:.0f}%)' if whole else 'no judged turn'


def live_findings(data):
    return [{'kind': 'skill_retired_corrected_above_base', 'project': row['project'], 'evidence': row}
            for row in data.get('skill_live') or [] if row['retired_reason'] == 'corrected_above_base']


def render_skill_live(data):
    rows = data.get('skill_live')
    lines = ['', '## Live use of shipped skills', '']
    if rows is None:
        return lines + ['Not available: the proxy has not applied the migration that adds `memory.skill_live_outcomes`.']
    if not rows:
        return lines + ['No shipped skill has a live measurement yet. The promoter records one on each run.']
    lines += ['Loads are Skill tool uses of the shipped folder in its project, counted from the day it shipped (at most 30 days back). '
              'The corrected rate is the share of the judged operator turns that loaded the skill which the judge marked corrected; '
              "the base rate is the same share over all of the project's judged turns in the same window. "
              'The promoter retires a skill with at least 10 loads whose corrected rate is above the base rate.', '',
              '| Project | Skill | Shipped | Loads | Corrected after a load | Base corrected rate | Status |',
              '|---|---|---|---:|---|---|---|']
    for r in rows:
        status = f"retired ({r['retired_reason']})" if r['retired_at'] else 'shipped'
        lines.append(f"| {r['project']} | `{r['folder']}` | {r['first_shipped_on']} | {r['loads']} | "
                     f"{rate_text(r['corrected_turns'], r['judged_turns'])} | "
                     f"{rate_text(r['base_corrected_turns'], r['base_judged_turns'])} | {status} |")
    return lines


WORKER_TOGGLE_GRACE_DAYS = 2


def llm_findings(data):
    out = []
    for row in data.get('worker_toggles', []):
        if not row['enabled'] and float(row['days_in_state']) >= WORKER_TOGGLE_GRACE_DAYS:
            out.append({'kind': 'worker_toggle_off', 'project': 'global', 'evidence': row})
    for row in data.get('llm_roles', []):
        if row['last_outcome'] != 'ok':
            out.append({'kind': 'llm_role_failing', 'project': 'global', 'evidence': row})
    return out


def render_proposer_days(data):
    days = data.get('proposer_active_days')
    if days is None:
        return []
    lines = [f"- Days with at least one lesson proposer call, UTC month {r['month']}: {r['days_with_calls']}." for r in days]
    return lines or ['- No lesson proposer call in the last three UTC months.']


def opportunity(row):
    return {**row, 'status': 'candidate_requires_validation',
            'next_action': 'Inspect original tool input/result; match an existing recipe before creating one; validate dependencies and invalidation before reuse.',
            'evidence_url': f"http://localhost:9989/api/requests/{row['example_request_id']}/tool-calls",
            'savings_status': 'Repeated bytes are observed, not proven avoidable tokens.'}


def render_memory_grammar(data):
    if 'memory_update_tags' not in data:
        return []
    totals = memory_grammar_totals(data)
    lines = ['', '## Memory verb grammar', '',
             'Sessions are taught two tags in the session-start topic catalog: `<mem:recall topic="X"/>` pulls the '
             'project lessons on a topic into the next request, and `<mem:update topic="X" kind="...">rule</mem:update>` '
             'records a lesson without a tool call. A tag counts only when a finished reply or request carried it.',
             '',
             f"- `<mem:recall>` tags: {totals['recall_tag_requests']} requests carried one; "
             f"{totals['recall_injected']} led to lessons being delivered.",
             f"- `<mem:update>` tags: {totals['update_tags_found']} found in replies; "
             f"{totals['update_lessons_written']} stored as lessons and {totals['update_lessons_skipped']} skipped "
             f"(a duplicate of an existing lesson, or a failed write).",
             f"- Lessons with source `harvester` created this window: {totals['harvested_lessons']}."]
    recall = {r['project']: r for r in data.get('memory_recall_tags', [])}
    update = {r['project']: r for r in data.get('memory_update_tags', [])}
    if recall or update:
        lines += ['', '| Project | Recall tags | Recall delivered | Update tags found | Lessons stored | Lessons skipped |',
                  '|---|---:|---:|---:|---:|---:|']
        for name in sorted(set(recall) | set(update)):
            r, u = recall.get(name, {}), update.get(name, {})
            lines.append(f"| {name} | {r.get('requests_with_recall_tags', 0)} | "
                         f"{r.get('injected', 0) + r.get('delivered_out_of_band', 0)} | {u.get('tags_found', 0)} | "
                         f"{u.get('lessons_written', 0)} | {u.get('lessons_skipped', 0)} |")
    return lines


def render_lesson_quality(data):
    if 'lesson_shape' not in data:
        return []
    shapes = data['lesson_shape']
    if shapes is None:
        return ['', '## Lesson quality', '',
                'Not available: the database does not have the lesson shape table yet, because the migration '
                '20261014190000_lesson_shape_findings has not run. Start the proxy to apply it.']
    active =sum(r['lessons'] for r in shapes)
    chars = sum(r['stored_chars'] or 0 for r in shapes)
    injected = {r['shape']: r for r in data.get('lesson_shape_injected', [])}
    injected_chars = sum(r['injected_chars'] or 0 for r in injected.values())
    lines = ['', '## Lesson quality', '',
             'A rule is a durable instruction with its trigger and what to do. A status lesson reports on a session. '
             'An over-length lesson exceeds the `lessons.shape.max_chars` setting. "Unchecked" lessons have not been '
             'reached by the shape check yet. Counts cover active daily and long-term lessons; injected characters '
             'cover every injection in the window, including lessons archived since.', '',
             f"- Active lessons: {active}; mean length {round(chars / active) if active else 0} characters; "
             f"{sum(r['over_600_chars'] for r in shapes)} over 600 characters; "
             f"{sum(r['under_80_chars'] for r in shapes)} under 80 characters.", '',
             '| Shape | Active lessons | Stored characters | Injections | Injected characters | Share of injected characters |',
             '|---|---:|---:|---:|---:|---:|']
    for row in shapes:
        sent = injected.get(row['shape'], {})
        share = (sent.get('injected_chars') or 0) / injected_chars if injected_chars else 0
        lines.append(f"| {row['shape']} | {row['lessons']} | {row['stored_chars'] or 0} | "
                     f"{sent.get('injections', 0)} | {sent.get('injected_chars') or 0} | {share:.1%} |")
    for row in data.get('lesson_shape_pending', []):
        lines.append(f"- Waiting for a rewrite: {row['lessons']} `{row['shape']}` lessons, oldest found {row['oldest_found_at']}.")
    resolved = ', '.join(f"{r['lessons']} {r['resolution']}" for r in data.get('lesson_shape_resolved', []))
    lines.append(f"- Resolved this window: {resolved or 'none'}.")
    for row in data.get('lesson_relations_write_time', []):
        lines.append(f"- Relations `{row['method']}` `{row['kind']}` `{row['status']}`: {row['relations']} "
                     f"({row['applied']} applied by consolidation).")
    effect = {r['side']: r for r in data.get('lesson_replacement_effect', [])}
    for side in ('replaced', 'replacement'):
        if side in effect:
            row = effect[side]
            rate = row['corrected_turns'] / row['turns'] if row['turns'] else 0
            lines.append(f"- Turns that received the {side} lesson: {row['turns']}, {row['corrected_turns']} corrected "
                         f"({rate:.1%}). A rate difference over a few turns is a reason to read the pair, not evidence of harm.")
    return lines


def skills_by_project(data):
    """Mike-written skills this machine's promoter still ships, per project, from its ledger."""
    names = {row['project_uuid']: row['project_path'] for row in data.get('project_names', [])}
    counts = collections.Counter(key.split(':', 1)[0] for key, entry in skill_ledger().items() if not entry['retired_at'])
    return sorted((names.get(uuid, uuid), count) for uuid, count in counts.items())


def skill_cap(data):
    rows = data.get('skill_cap_setting') or []
    value = rows[0]['value'] if rows else None
    return value if isinstance(value, int) and not isinstance(value, bool) else 12


def render_placement(data):
    if 'lesson_placement' not in data:
        return []
    placed = data['lesson_placement']
    if placed is None:
        return ['', '## Where lessons belong', '',
                'Not available: the database does not have the lesson placement columns yet, because the migration '
                '20261014200000_lesson_placement has not run. Start the proxy to apply it.']
    total = sum(r['lessons'] for r in placed)
    stuck = (data.get('lesson_placement_stuck') or [{'lessons': 0}])[0]['lessons']
    lines = ['', '## Where lessons belong', '',
             'Each active lesson has one home: a rule (a short constraint tied to files, proposed to the operator), '
             'a skill (a procedure for a recognisable situation; only these feed skill drafts), an agent definition '
             '(a persona, proposed to the operator), memory (a fact or correction recalled when relevant), or nowhere '
             '(already covered, or not worth keeping). "Unplaced" lessons have not been reached by the placement pass yet.', '',
             '| Placement | Active lessons | Agent-scoped | Naming a covering document |', '|---|---:|---:|---:|']
    lines += [f"| {r['placement']} | {r['lessons']} | {r['agent_scoped']} | {r['naming_a_document']} |" for r in placed]
    lines.append(f"- Active lessons: {total}; {stuck} could not be placed after three attempts.")
    cap = skill_cap(data)
    skills = skills_by_project(data)
    lines += ['', f'Mike-written skills against the cap of {cap} per project (setting `lessons.skills.max_per_project`; '
              'counted from this machine\'s promotion ledger):', '']
    if skills:
        lines += ['| Project | Skills | Cap | State |', '|---|---:|---:|---|']
        lines += [f"| {name} | {count} | {cap} | {'over the cap' if count > cap else 'within the cap'} |" for name, count in skills]
    else:
        lines.append('- No mike-written skill is shipped on this machine.')
    for row in data.get('lesson_placement_proposals') or []:
        scope = f", agent {row['agent']}" if row['agent'] else ''
        covered = f" Covered by or belongs in: {row['covered_by']}." if row['covered_by'] else ''
        lines += ['', f"### Proposed {row['placement']} entry: lesson {row['id']} ({row['project']}{scope})", '',
                  row['text'], '', f"Why: {row['reason'] or 'no reason recorded'}.{covered}"]
    return lines


def render(report):
    totals = report['totals']
    lines = ['# Weekly Mike learning audit', '',
             f"Window: {report['start']} inclusive to {report['end']} exclusive.", '',
             f"{totals['sessions']} sessions; {totals['requests']} requests; {totals['tool_calls']} tool calls; {totals['missing_usage']} requests lack input usage.",
             '', 'Input, output, cache-read and cache-write counts are reported separately in report.json. '
             'Repeated results are candidates, not guaranteed savings; accepted labels are not independent proof of task completion.',
             '', '## Learning health', '']
    lines += [f"- {f['project']}: {f['kind']}" for f in report['findings']] or ['No learning-health findings.']
    lines += ['', '## Current learning pipeline', '',
              'These are current operational snapshots, separate from the historical activity window. '
              'Reserved attempts include failures and uncertain interrupted calls; they are not paid-call counts.', '']
    for row in report['data'].get('proposal_jobs', []):
        lines.append(f"- Proposal jobs `{row['status']}`: {row['jobs']}; oldest {row['oldest_created_at']}; maximum retries {row['max_retries']}.")
    for row in report['data'].get('proposal_recovery', []):
        lines.append(f"- Historical recovery `{row['recovery_state']}`: {row['jobs']}.")
    for row in report['data'].get('proposer_budget', []):
        lines.append(f"- UTC {row['day']}: {row['consumed']} reserved attempts; {row['recorded_calls']} recorded proposer calls.")
    lines += render_proposer_days(report['data'])
    publication = report.get('publication', {'state': 'missing'})
    lines.append(f"- Memory/skill publication: `{publication['state']}`; "
                 f"{publication.get('lessons', 0)} lessons, {publication.get('memory_projects', 0)} projects, "
                 f"{publication.get('skill_installations', 0)} installations; updated {publication.get('updated_at', 'unknown')}.")
    skill_loads = generated_skill_loads(report['data'])
    generated_loads = [r for r in skill_loads if r['generated']]
    lines.append(f"- Skill loads this window: {sum(r['loads'] for r in skill_loads)} total, "
                 f"{sum(r['loads'] for r in generated_loads)} of mike-generated skills.")
    lines += [f"  - `{r['skill']}`: {r['loads']} loads across {r['sessions']} sessions." for r in generated_loads]
    lines += render_memory_grammar(report['data'])
    lines += render_lesson_quality(report['data'])
    lines += render_placement(report['data'])
    if 'lesson_delivery' in report:
        lines += delivery.render(report['lesson_delivery'])
    lines += render_skill_live(report['data'])
    lines += ['', 'Stored governor overrides (omitted settings use the running version’s defaults):', '']
    lines += [f"- `{r['key']}`: `{json.dumps(r['value'])}`" for r in report['data'].get('governor_settings', [])]
    lines += ['', '## Projects', '', '| Project | Sessions | Requests | Tool calls | Errors |', '|---|---:|---:|---:|---:|']
    for name, row in sorted(report['projects'].items()):
        lines.append(f"| {name} | {row['sessions']} | {row['requests']} | {row['tool_calls']} | {row['tool_errors']} |")
    lines += ['', '## Reuse opportunities', '',
              'Full ranked candidates and stable source request IDs are in opportunities.json. '
              'Discover validated recipes with `scripts/tool-reuse list`; use `show ID` for contract and `run ID` for a fresh-or-reused result. '
              'Never execute a captured command merely because it repeated.', '',
              '## Follow-through', '',
              'Compare with the prior window in report.json. Fix ingestion/delivery failures first. '
              'For each repeated investigation, inspect its source, search the recipe registry, validate a reusable command, '
              'and measure recipe hit/miss/error telemetry next week. Judge quality using relevant subsequent tasks, not project-wide correlation.']
    lines += ['', '## Repeated tool/result evidence', '',
              '| Project | Tool | Calls / sessions | Repeated bytes | Source request / tool |',
              '|---|---|---:|---:|---|']
    for item in report['opportunities']:
        lines.append(f"| {item['project']} | {item['tool_name']} | {item['calls']} / {item['sessions']} | {item['repeated_result_bytes']} | {item['example_request_id']} / {item['example_tool_use_id']} |")
    lines += ['', '## Helper creation across sessions', '']
    for item in report['data']['created_tools']:
        lines.append(f"- {item['project']}: `{item['file_path']}`, {item['writes']} writes across {item['sessions']} sessions; source request {item['example_request_id']}. Inspect whether these were necessary edits or recreation.")
    return '\n'.join(lines) + '\n'


def query_if_migrated(sql):
    try:
        return query(sql)
    except RuntimeError as error:
        if 'does not exist' in str(error):
            return None
        raise


def run_queries(start, end):
    return {name: query_if_migrated(sql) if name in MIGRATION_DEPENDENT else query(sql)
            for name, sql in queries(start, end).items()}


def build(start, end):
    data = run_queries(start, end)
    projects = collections.defaultdict(lambda: {'sessions': 0, 'requests': 0, 'tool_calls': 0, 'tool_errors': 0})
    for row in data['sessions']:
        if not row['is_global']:
            projects[row['project']]['sessions'] = row['sessions']
    for row in data['requests']:
        projects[row['project']]['requests'] += row['requests']
    for row in data['tools']:
        projects[row['project']]['tool_calls'] += row['calls']
        projects[row['project']]['tool_errors'] += row['errors']
    return {'version': 1, 'start': start, 'end': end, 'data': data, 'projects': dict(projects),
            'totals': {'sessions': next(r['sessions'] for r in data['sessions'] if r['is_global']),
                       'requests': sum(r['requests'] for r in data['requests']),
                       'tool_calls': sum(r['calls'] for r in data['tools']),
                       'missing_usage': sum(r['missing_usage'] for r in data['requests'])},
            'findings': findings(data),
            'opportunities': [opportunity(r) for r in data['repeated_results']]}


def run(args):
    end = dt.datetime.fromisoformat(args.end.replace('Z', '+00:00')) if args.end else dt.datetime.now(dt.timezone.utc)
    if end.tzinfo is None:
        raise ValueError('--end must include a timezone')
    start = end - dt.timedelta(days=7)
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    previous_path = output / 'latest.json'
    previous = json.loads(previous_path.read_text()) if previous_path.exists() else None
    report = build(start.isoformat(), end.isoformat())
    report['generated_at'] = dt.datetime.now(dt.timezone.utc).isoformat()
    report['publication'] = publication_status()
    if report['publication']['state'] != 'published':
        report['findings'].append({'kind': 'memory_publication_' + report['publication']['state'],
                                   'project': 'global', 'evidence': report['publication']})
    url = 'http://127.0.0.1:9989/api/improvement/exposures?' + urllib.parse.urlencode({'from': start.isoformat(), 'until': end.isoformat()})
    with urllib.request.urlopen(url, timeout=120) as response:
        report['exposures'] = json.load(response)
    report['lesson_delivery'] = delivery.fetch(start.isoformat(), end.isoformat())
    report['previous_window'] = ({'start': previous['start'], 'end': previous['end'],
        'totals': previous['totals'], 'delta': {k: v - previous['totals'].get(k, 0)
          for k, v in report['totals'].items()}} if previous else None)
    telemetry = Path(os.environ.get('MIKE_TOOL_REUSE_STATE', Path.home() / '.local/state/mike-tool-reuse')) / 'events.jsonl'
    report['reuse_events'] = [json.loads(line) for line in telemetry.read_text().splitlines()
        if line and start <= dt.datetime.fromisoformat(json.loads(line)['at']) < end] if telemetry.exists() else []
    target = output / end.strftime('%Y-%m-%dT%H%M%SZ')
    target.mkdir(exist_ok=True)
    for name, value in [('report.json', report), ('opportunities.json', report['opportunities'])]:
        (target / name).write_text(json.dumps(value, indent=2) + '\n')
    (target / 'report.md').write_text(render(report))
    tmp = output / '.latest.json.tmp'
    tmp.write_text(json.dumps(report, indent=2) + '\n')
    os.replace(tmp, previous_path)
    from tool_reuse.core import export
    export(output / 'tool-recipes.md')
    if args.sync:
        sync_reports(output)
    print(json.dumps({'report': str(target / 'report.md'), 'findings': len(report['findings']),
                      'reuse_candidates': len(report['opportunities'])}))
