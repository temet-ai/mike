"""Materialize accepted project lessons and validated recipes for future agents."""
import datetime
import hashlib
import json
from pathlib import Path
import re

from knowledge.collector import Client, atomic_write, read_document
from knowledge.machine import machine_identity
from . import live
from .report import ROOT, query
from .skill_budget import enforce_cap, hold_overlapping, read_cap
from tool_reuse.core import catalog

MARKER = '<!-- mike-learning-generated:v1 -->'


def write_owned(path, body, manifest):
    path = Path(path)
    previous = manifest.get(str(path))
    if path.exists() and hashlib.sha256(path.read_bytes()).hexdigest() != previous:
        if path.read_text() == body:
            manifest[str(path)] = hashlib.sha256(body.encode()).hexdigest()
            return
        raise ValueError(f'Generated memory/skill was edited or is not owned: {path}; preserve and reconcile it')
    atomic_write(path, body)
    manifest[str(path)] = hashlib.sha256(body.encode()).hexdigest()


def slug(text):
    return re.sub(r'[^a-z0-9]+', '-', text.lower()).strip('-')[:40] or 'project'


def memory_body(project, rows):
    lines = [MARKER, f'# {project} — learned project memory', '',
             'Paths in this file were recorded on the machine that learned them; match the project by its folder name and checkout, not by absolute path.', '',
             'These are accepted project lessons, with source IDs. Apply only lessons relevant to the current task. '
             'Current user instructions take precedence. Judge candidates awaiting review are excluded.', '']
    for row in rows:
        lines += [f"## Lesson {row['id']}: {row['kind']} ({row['topic'] or 'project'})", '', row['text'], '',
                  f"Source: lesson {row['id']}; turn {row['source_turn_id']}; stage {row['stage']}; source {row['source']}.", '']
    return '\n'.join(lines)


def subject(project):
    if project.startswith('explicitly shared'):
        return 'explicitly shared learned workflows matching the current task'
    path, _, agent = project.partition(', agent ')
    name = f'the `{Path(path).name or path}` project, in whichever folder it is checked out on this machine'
    return name + (f', agent {agent}' if agent else '')


SPAN = re.compile(r'`([^`\n]{3,40})`')
PATH = re.compile(r'(?<![\w/.-])((?:[\w.-]+/)+[\w.-]+|[\w-]+[.](?:sh|py|rs|toml|sql|md|json|ya?ml|ts|svelte))(?![\w/-])')


def triggers(rows, limit=3):
    """Commands and paths the lessons mention, most repeated first, first seen first on ties."""
    seen = {}
    for row in rows or []:
        for found in SPAN.findall(row['text']) + PATH.findall(row['text']):
            found = found.strip().rstrip('.,;:')
            if (found and not found.startswith(('http', '/', '~', '$')) and len(found.split()) <= 4
                    and re.search(r'[^A-Za-z0-9]', found)):
                seen[found] = seen.get(found, 0) + 1
    return sorted(seen, key=lambda item: -seen[item])[:limit]


def trigger_clause(rows):
    found = triggers(rows)
    return 'Use when running or touching ' + ', '.join(f'`{item}`' for item in found) + '. ' if found else ''


def project_skill(name, project, memory, rows=None):
    description = f'{trigger_clause(rows)}Use learned project conventions and workflows when working on {subject(project)}; retrieve the relevant accepted lessons before repeating an investigation or verification.'
    return f'''---
name: {name}
description: {json.dumps(description)}
---
{MARKER}

Use this skill only for {subject(project)}. Any absolute path in the memory was recorded on another machine. Search [accepted project memory](references/memory.md) for the current task, then read the matching lessons with their provenance. Do not load every lesson by default. Source IDs support checking newer evidence through Mike's lesson and project-knowledge tools.

Use corrections as scoped project guidance, not authority to expand the user's request. Prefer an existing validated script or recipe when a lesson describes repeated work. Check its input/freshness contract and original exit code before relying on a saved result. A changed dependency requires fresh verification.

The memory reference is generated from accepted lessons. Pending judge proposals are excluded. Updates preserve manually edited files as conflicts rather than replacing them.
'''


def topic_skill(name, project, topic, rows):
    folder = Path(project).name
    title = topic.replace('-', ' ').replace('_', ' ').capitalize()
    kinds = ', '.join(sorted({row['kind'] for row in rows}))
    agent = '' if rows[0].get('agent_id') is None else f" Applies to the {rows[0]['agent_name'] or rows[0]['agent_id']} agent only."
    found = triggers(rows)
    situation = (f'Use when running or touching {", ".join(f"`{item}`" for item in found)}, or when the task involves {title.lower()}.'
                 if found else f'Use when the task involves {title.lower()}.')
    description = f"{title}: {len(rows)} accepted lessons for the `{folder}` project, kinds {kinds}. {situation}{agent}"
    lines = ['---', f'name: {name}', f'description: {json.dumps(description)}', '---', MARKER, '',
             f'Use only for the `{folder}` project. Paths were recorded on the machine that learned them.', '',
             'The lessons are in [references/lessons.md](references/lessons.md).', '']
    return '\n'.join(lines)


def lessons_reference(project, folder, rows):
    lines = [f'Lessons behind the {folder} skill for the `{Path(project).name}` project, each with its source turn.', MARKER, '']
    for row in rows:
        lines += [f"## Lesson {row['id']}: {row['kind']}", '', row['text'], '',
                  f"Source: lesson {row['id']}; turn {row['source_turn_id']}; stage {row['stage']}; source {row['source']}.", '']
    return '\n'.join(lines)


def skill_folder(topic_slug, rows):
    """The topic's folder, named for the agent only when every lesson in it belongs to that one agent."""
    agents = {(row.get('agent_id'), row.get('agent_name')) for row in rows}
    if len(agents) == 1:
        agent_id, agent_name = next(iter(agents))
        if agent_id is not None:
            return (topic_slug + '-' + slug(agent_name or str(agent_id)))[:63]
    return topic_slug[:63]


def decoded(value):
    return json.loads(value) if isinstance(value, str) else value


REFERENCE_NAME = re.compile(r'[a-z0-9]+(-[a-z0-9]+)*')


def drafted_references(draft):
    for reference in decoded(draft.get('reference_files')) or []:
        name = reference.get('name', '')
        if name != 'lessons' and len(name) <= 64 and REFERENCE_NAME.fullmatch(name):
            yield name, reference['content']


def local_checkout(project_path, roots=()):
    """The project's checkout on this machine: the recorded path when it is here, else a location this machine registered."""
    for candidate in [project_path, *roots]:
        if eligible_checkout(candidate):
            return candidate
    return None


def eligible_checkout(project_path):
    path = Path(project_path)
    if not path.is_absolute() or (Path.home() / '.local/state') in path.parents or '/tmp/' in f'{path}/':
        return False
    return path.is_dir() and (path / '.git').exists()


def exclude_file(checkout):
    git = checkout / '.git'
    try:
        if git.is_dir():
            return git / 'info/exclude'
        text = git.read_text().strip()
        if not text.startswith('gitdir:'):
            return None
        gitdir = (checkout / text[len('gitdir:'):].strip()).resolve()
        commondir = gitdir / 'commondir'
        common = (gitdir / commondir.read_text().strip()).resolve() if commondir.exists() else gitdir
        return common / 'info/exclude'
    except OSError:
        return None


def add_exclude_lines(path, lines):
    text = path.read_text() if path.exists() else ''
    have = set(text.splitlines())
    missing = [line for line in dict.fromkeys(lines) if line not in have]
    if missing:
        if text and not text.endswith('\n'):
            text += '\n'
        atomic_write(path, text + ''.join(f'{line}\n' for line in missing))


def recipe_skill(recipe):
    name = 'tool-' + recipe['id']
    runner = ROOT / 'scripts/tool-reuse'
    return name, f'''---
name: {name}
description: {json.dumps(recipe['description'] + '. Applies to ' + recipe['cwd'] + '.')}
---
{MARKER}

Use the validated `{recipe['id']}` procedure when its purpose matches the task.

Contract: {recipe['contract']}

Inspect the current contract with `{runner} show {recipe['id']}`. Run `{runner} run {recipe['id']}`; it returns a compact receipt and paths to full stdout/stderr. Read only the result relevant to the question. Preserve the command's exit code. Use `--fresh` when current conditions are not covered by the recipe's declared dependencies.

Do not recreate this helper in a new session. If the procedure is inadequate, improve and validate the existing recipe, preserving its scope and provenance.
'''


def promote(sync=True, skill_roots=None):
    memory_root = ROOT / 'artifacts/learning-memory'
    state = Path.home() / '.local/state/mike-learning-promotion'
    state.mkdir(parents=True, exist_ok=True)
    manifest_path = state / 'manifest.json'
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
    ledger_path = state / 'skill-installs.json'
    ledger = json.loads(ledger_path.read_text()) if ledger_path.exists() else {}
    skill_roots = skill_roots or [Path.home() / '.codex/skills', Path.home() / '.claude/skills']
    rows = query("""SELECT l.id,l.text,l.topic,l.source_turn_id,st.code AS stage,src.code AS source,
        k.code AS kind,p.project_uuid,p.project_path,l.agent_id,a.name AS agent_name,pl.code AS placement,
        l.placement_covered_by AS covered_by
        FROM memory.lessons l JOIN projects.projects p ON p.id=l.project_id
        JOIN memory.lesson_stages st ON st.id=l.stage_id
        JOIN memory.lesson_sources src ON src.id=l.source_id
        JOIN memory.lesson_kinds k ON k.id=l.kind_id
        LEFT JOIN memory.lesson_placements pl ON pl.id=l.placement_id
        LEFT JOIN agents.agents a ON a.id=l.agent_id
        WHERE l.archived_at IS NULL AND (l.expires_at IS NULL OR l.expires_at>now())
        AND (st.code IN ('daily','long_term') OR (st.code='raw' AND src.code='operator'))
        AND p.project_path <> '(no project)'
        AND NOT EXISTS (SELECT 1 FROM memory.lesson_relations rel
            JOIN memory.lesson_relation_kinds rk ON rk.id=rel.kind_id AND rk.code='supersedes'
            JOIN memory.lesson_relation_statuses rs ON rs.id=rel.status_id AND rs.code='confirmed'
            WHERE rel.to_lesson_id=l.id)
        ORDER BY p.project_uuid,l.id""")
    accepted, approved, rejected = {}, {}, {}
    by_review = {'approved': approved, 'rejected': rejected}
    for row in query("""SELECT d.id, p.project_uuid, d.folder, d.lesson_watermark, d.revision, d.review,
       d.skill_md, d.reference_files, d.lesson_ids
  FROM memory.skill_drafts d JOIN projects.projects p ON p.id = d.project_id
 WHERE (d.status = 'accepted' AND d.review IS NULL) OR d.review IN ('approved', 'rejected')
 ORDER BY d.project_id, d.folder, d.lesson_watermark DESC, d.revision DESC"""):
        key = f"{row['project_uuid']}:{row['folder']}"
        by_review.get(row.get('review'), accepted).setdefault(key, row)
    policies = {row['project_uuid']: row['mode'] for row in query("""SELECT p.project_uuid, sp.mode
  FROM memory.skill_policies sp JOIN projects.projects p ON p.id = sp.project_id""")}
    user_retired = {f"{row['project_uuid']}:{row['folder']}" for row in query("""SELECT p.project_uuid, r.folder
  FROM memory.skill_retirements r JOIN projects.projects p ON p.id = r.project_id""")}
    projects = {}
    for row in rows:
        projects.setdefault(row['project_uuid'], []).append(row)
    client = Client('http://127.0.0.1:9989')
    machine = machine_identity()
    locations = {}
    for row in query(f"""SELECT p.project_uuid, l.root_path FROM projects.locations l
        JOIN projects.projects p ON p.id=l.project_id
        WHERE l.machine_uuid='{machine['machine_uuid']}' ORDER BY l.last_seen DESC"""):
        locations.setdefault(row['project_uuid'], []).append(row['root_path'])
    known_projects = {r['project_uuid']: r['project_path'] for r in query('SELECT project_uuid,project_path FROM projects.projects')}
    for directory in memory_root.glob('*'):
        if directory.is_dir() and directory.name in known_projects:
            projects.setdefault(directory.name, [])
    desired = {}
    project_documents = {}
    topic_candidates = {}
    for project_id, lessons in projects.items():
        project = known_projects[project_id]
        directory = memory_root / project_id
        groups = {None: []}
        for row in lessons:
            groups.setdefault(row.get('agent_id'), []).append(row)
        documents = []
        for agent, scoped in groups.items():
            suffix = '' if agent is None else f'-agent-{agent}'
            scope = project if agent is None else f"{project}, agent {scoped[0]['agent_name']} only"
            body = memory_body(scope, scoped)
            memory = directory / f'memory{suffix}.md'
            desired[str(memory)] = body
            documents.append(memory)
        project_documents[project_id] = documents
        topics = {}
        for row in lessons:
            topic = row.get('topic')
            if not topic or not topic.strip() or topic.startswith(('shared:', 'verifier:')) or not feeds_topic_skill(row, slug(topic)):
                continue
            topics.setdefault(slug(topic), []).append(row)
        for topic_slug, topic_rows in topics.items():
            if len(topic_rows) >= 2:
                folder = skill_folder(topic_slug, topic_rows)
                topic_candidates[f'{project_id}:{folder}'] = (project, folder, topic_rows, locations.get(project_id, []))
        keep_reviewed(project_id, project, lessons, accepted, approved, topic_candidates, locations.get(project_id, []))
    shared = [row for row in rows if row.get('agent_id') is None and (row.get('topic') or '').startswith('shared:')]
    if shared:
        name = 'shared-workflows'
        body = memory_body('Explicitly shared workflows; each lesson retains its own applicability', shared)
        skill = project_skill(name, 'explicitly shared learned workflows matching the current task', None, shared)
        for skills in skill_roots:
            desired[str(skills / name / 'references/memory.md')] = body
            desired[str(skills / name / 'SKILL.md')] = skill
    recipe_excludes = {}
    for recipe in catalog():
        name, body = recipe_skill(recipe)
        checkout = Path(recipe['cwd'])
        if not eligible_checkout(checkout):
            continue
        for skills in ('.claude/skills', '.codex/skills'):
            desired[str(checkout / skills / name / 'SKILL.md')] = body
            recipe_excludes.setdefault(checkout, []).append(f'{skills}/{name}/')
    today = datetime.date.today()
    loads = {}
    for row in query("""SELECT p.project_uuid, u.input->>'skill' AS skill, count(*) AS loads
        FROM tools.uses u JOIN requests.requests r ON r.id=u.request_id
        LEFT JOIN sessions.sessions s ON s.id=r.session_id
        LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
        WHERE u.tool_name='Skill' AND u.created_at > now() - interval '30 days'
        GROUP BY 1, 2"""):
        if row['skill']:
            loads[f"{row['project_uuid']}:{row['skill']}"] = int(row.get('loads', 1))
    cap = read_cap(query)
    shipped = {key for key in topic_candidates if key in accepted or key in approved or (key in ledger and not ledger[key]['retired_at'])}
    purpose_held = hold_overlapping(
        {key: (candidate[2][0]['topic'], len(candidate[2])) for key, candidate in topic_candidates.items()}, shipped)
    emitted, skipped, held_retired = {}, set(), []
    held_by_user, held_for_approval = [], []
    authored_keys, extra, fresh = set(), {}, set()
    for key, (recorded, folder, topic_rows, roots) in topic_candidates.items():
        if key in purpose_held:
            continue
        project = local_checkout(recorded, roots)
        if project is None:
            skipped.add(recorded)
            continue
        watermark = max(row['id'] for row in topic_rows)
        entry = ledger.get(key)
        if key in user_retired:
            hold(ledger, key, 'user', today)
            held_by_user.append(key)
            continue
        if entry and entry['retired_at'] and entry.get('retired_reason') in (None, 'unloaded', 'corrected_above_base') \
                and watermark <= entry['lesson_watermark']:
            held_retired.append(key)
            continue
        mode = policies.get(key.partition(':')[0], 'ask')
        draft = chosen_draft(mode, watermark, accepted.get(key), approved.get(key), rejected.get(key))
        if mode == 'auto' and draft is None and key in rejected:
            hold(ledger, key, 'user', today)
            held_by_user.append(key)
            continue
        if mode != 'auto' and draft is None:
            hold(ledger, key, 'policy', today)
            held_for_approval.append(key)
            continue
        if entry is None or entry['retired_at']:
            entry = ledger[key] = {'first_emitted': today.isoformat(), 'retired_at': None}
            fresh.add(key)
        entry['lesson_watermark'] = watermark
        checkout = Path(project)
        skill_dir = checkout / '.claude/skills' / folder
        path = skill_dir / 'SKILL.md'
        emitted[key] = (checkout, folder, path)
        drafted = draft is not None
        entry.update(path=str(path), draft_id=draft.get('id') if drafted else None, retired_reason=None)
        desired[str(path)] = draft['skill_md'] if drafted else topic_skill(folder, project, topic_rows[0]['topic'], topic_rows)
        desired[str(skill_dir / 'references/lessons.md')] = lessons_reference(project, folder, topic_rows)
        extra[key] = [skill_dir / 'references/lessons.md']
        if drafted:
            authored_keys.add(key)
            for name, content in drafted_references(draft):
                desired[str(skill_dir / 'references' / f'{name}.md')] = content
                extra[key].append(skill_dir / 'references' / f'{name}.md')
    measured = live.measure(query, ledger, list(emitted))
    paths = {key: path for key, (_, _, path) in emitted.items()}
    retired = retire_unloaded(desired, ledger, loads, today, paths)
    harmful = live.retire_corrected_above_base(desired, ledger, measured, today,
                                               {key: path for key, path in paths.items() if key not in retired})
    live.record(ledger, measured)
    for key in [*retired, *harmful]:
        withdraw(key, emitted, extra, desired)
    capped, held_for_cap = enforce_cap(ledger, loads, today, set(emitted), fresh,
                                       {key: len(topic_candidates[key][2]) for key in emitted}, cap,
                                       authored_keys & emitted.keys())
    for key in capped:
        ledger[key]['retired_at'] = today.isoformat()
    for key in capped + held_for_cap:
        withdraw(key, emitted, extra, desired)
    retired += capped
    authored = len(authored_keys & emitted.keys())
    for key in list(ledger):
        if key not in emitted and not ledger[key]['retired_at']:
            del ledger[key]
    conflicts = reconcile(desired, manifest, state)
    excludes, exclude_skipped = recipe_excludes, []
    for checkout, folder, path in emitted.values():
        if str(path) not in conflicts:
            excludes.setdefault(checkout, []).append(f'.claude/skills/{folder}/')
    for checkout, lines in excludes.items():
        target = exclude_file(checkout)
        if target is None:
            exclude_skipped.append(str(checkout))
        else:
            add_exclude_lines(target, lines)
    if sync:
        for project_id, documents in project_documents.items():
            directory = memory_root / project_id
            if any(str(path) in conflicts for path in documents):
                continue
            try:
                location = client.request('/api/knowledge/locations', {'project': project_id,
                    **machine, 'root_path': str(directory)})
                source = client.request('/api/knowledge/sources', {'location_uuid': location['location_uuid'],
                    'name': 'accepted-learning-memory', 'root_path': str(directory)})
                client.request(f"/api/knowledge/sources/{source['source_uuid']}/scan", {
                    'generation': source['generation'], 'documents': [read_document(path, directory) for path in documents]})
            except RuntimeError as error:
                conflicts[f'knowledge:{project_id}'] = str(error)
    published = live.publish(client, ledger) if sync else {'sent': 0, 'error': None}
    atomic_write(ledger_path, json.dumps(ledger, indent=2) + '\n')
    atomic_write(manifest_path, json.dumps(manifest, indent=2) + '\n')
    install_report = report_installs(client, machine, ledger) if sync else None
    result = {'lessons': len(rows), 'memory_projects': len(projects),
              'held_by_user': held_by_user, 'held_for_approval': held_for_approval,
              'install_report': install_report,
              'skills': [p for p in desired if p.endswith('/SKILL.md')], 'conflicts': conflicts,
              'topic_skills': len(emitted), 'topic_skill_projects': len({checkout for checkout, _, _ in emitted.values()}),
              'authored_skills': authored,
              'skipped_projects': sorted(skipped), 'exclude_skipped': exclude_skipped,
              'retired': retired, 'retired_corrected_above_base': harmful, 'held_retired': held_retired,
              'skill_cap': cap, 'capped_retired': capped, 'held_for_cap': held_for_cap,
              'purpose_held': purpose_held,
              'live_skills': len(measured), 'live_outcomes_sent': published['sent'],
              'live_publish_error': published['error']}
    atomic_write(state / 'latest.json', json.dumps(result, indent=2) + '\n')
    return result


def reconcile(desired, manifest, state):
    conflicts = {}
    for path, body in desired.items():
        try:
            write_owned(path, body, manifest)
        except ValueError as error:
            conflicts[path] = str(error)
    for old in list(manifest):
        if old in desired:
            continue
        path = Path(old)
        if path.exists():
            if hashlib.sha256(path.read_bytes()).hexdigest() != manifest[old]:
                conflicts[old] = f'Preserved manually edited retired file: {path}'
                continue
            archive = state / 'retired' / (hashlib.sha256(old.encode()).hexdigest() + '.md')
            atomic_write(archive, path.read_text())
            path.unlink()
            for directory in (path.parent, path.parent.parent):
                try:
                    directory.rmdir()
                except OSError:
                    break
        manifest.pop(old)
    return conflicts


def withdraw(key, emitted, extra, desired):
    _, _, path = emitted.pop(key)
    desired.pop(str(path), None)
    for extra_path in extra.pop(key, []):
        desired.pop(str(extra_path), None)


def retire_unloaded(desired, ledger, loads, today, emitted, days=30):
    retired = sorted(key for key in emitted
                     if not ledger[key]['retired_at'] and key not in loads
                     and (today - datetime.date.fromisoformat(ledger[key]['first_emitted'])).days >= days)
    for key in retired:
        desired.pop(str(emitted[key]), None)
        live.mark_retired(ledger[key], today, 'unloaded')
    return retired


def keep_reviewed(project_id, project, lessons, accepted, approved, topic_candidates, roots):
    """A reviewed skill (an accepted or approved draft) stays a candidate while any lesson it was
    drafted from is still active, whatever that lesson's placement now says. Placement decides
    where new content goes; it does not withdraw a skill a reviewer already passed."""
    by_id = {row['id']: row for row in lessons}
    for key, draft in {**accepted, **approved}.items():
        owner, _, folder = key.partition(':')
        if owner != project_id:
            continue
        current = topic_candidates.get(key)
        rows = list(current[2]) if current else []
        seen = {row['id'] for row in rows}
        rows += [by_id[i] for i in draft.get('lesson_ids') or [] if i in by_id and i not in seen]
        if rows:
            topic_candidates[key] = (project, folder, sorted(rows, key=lambda r: r['id']), roots)


def feeds_topic_skill(row, topic_slug):
    """A lesson feeds its topic's skill when it is placed as a skill, not placed yet, or placed
    nowhere because that same skill already covers it. Without the last case, every lesson the
    placement pass finds covered by an installed skill is taken out of that skill, and the
    skill is withdrawn once fewer than two lessons remain."""
    placement = row.get('placement')
    if placement in ('skill', None):
        return True
    if placement != 'nowhere':
        return False
    covered = Path((row.get('covered_by') or '').strip())
    return covered.name == 'SKILL.md' and covered.parent.name.startswith(topic_slug)


def chosen_draft(mode, watermark, accepted, approved, rejected=None):
    """The draft to ship: in ask mode only the user's approved one; in auto mode the newer of
    the approved draft and the accepted one. An accepted draft no newer than the one the user
    rejected is not shipped, so a rejection never brings back an earlier revision. A draft
    built before the topic's newest lesson keeps shipping until a newer one is accepted: the
    new lessons reach the skill through references/lessons.md, and the generated placeholder
    is worse than any reviewed draft."""
    if mode != 'auto':
        return approved
    rank = lambda d: (d['lesson_watermark'], d.get('revision', 1))
    if accepted and rejected and rank(rejected) >= rank(accepted):
        accepted = None
    found = [draft for draft in (approved, accepted) if draft]
    return max(found, key=rank, default=None)


def hold(ledger, key, reason, today):
    """Mark a folder written before as removed, for the reason it is no longer written."""
    entry = ledger.get(key)
    if entry and not entry['retired_at']:
        entry['retired_at'] = today.isoformat()
        entry['retired_reason'] = reason


def report_installs(client, machine, ledger):
    """Tell mike what this machine's checkouts hold, for the Skills page."""
    installs = []
    for key, entry in ledger.items():
        if not entry.get('path'):
            continue
        project_uuid, _, folder = key.partition(':')
        installs.append({'project_uuid': project_uuid, 'folder': folder, 'path': entry['path'],
                         'draft_id': entry.get('draft_id'), 'lesson_watermark': entry['lesson_watermark'],
                         'first_written': entry['first_emitted'], 'retired_at': entry['retired_at'],
                         'retired_reason': entry.get('retired_reason') if entry['retired_at'] else None})
    try:
        return client.request('/api/skills/installs', {'machine_uuid': machine['machine_uuid'],
                              'machine_name': machine['machine_name'], 'installs': installs})
    except (RuntimeError, OSError) as error:
        return {'error': str(error)}
