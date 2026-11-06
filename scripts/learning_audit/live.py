"""How the skills the promoter shipped do in real use: Skill loads and the judge's corrected rate on the turns that loaded them."""
import datetime
import re

MIN_LOADS_TO_RETIRE = 10
WINDOW_DAYS = 30
OUTCOMES_PATH = '/api/learning/skill-outcomes'

UUID = re.compile(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}')
FOLDER = re.compile(r'[a-z0-9]+(-[a-z0-9]+)*')
DAY = re.compile(r'\d{4}-\d{2}-\d{2}')

LIVE_SQL = """WITH shipped(project_uuid, folder, since) AS (VALUES {values}),
win AS (
  SELECT s.project_uuid, s.folder, p.id AS project_id,
         greatest(s.since::timestamptz, now() - interval '{days} days') AS window_start,
         now() AS window_end
    FROM shipped s JOIN projects.projects p ON p.project_uuid = s.project_uuid),
loads AS (
  SELECT w.project_uuid, w.folder, r.session_id, r.operator_turn_id
    FROM win w
    JOIN tools.uses u ON u.tool_name = 'Skill' AND u.input->>'skill' = w.folder
                     AND u.created_at >= w.window_start
    JOIN requests.requests r ON r.id = u.request_id
    LEFT JOIN sessions.sessions ss ON ss.id = r.session_id
   WHERE coalesce(r.project_id, ss.project_id) = w.project_id),
turns AS (
  SELECT t.id, t.project_id, t.created_at, t.reaction
    FROM memory.operator_turns t
    JOIN memory.turn_facts f ON f.operator_turn_id = t.id
   WHERE f.prompt_kind = 'genuine' AND f.text_chars > 0
     AND t.reaction IN ('accepted', 'corrected', 'redirected')
     AND t.created_at >= (SELECT min(window_start) FROM win)),
loaded AS (
  SELECT project_uuid, folder, count(*) AS loads,
         count(DISTINCT session_id) AS load_sessions,
         count(DISTINCT operator_turn_id) AS loaded_turns
    FROM loads GROUP BY project_uuid, folder),
judged AS (
  SELECT x.project_uuid, x.folder, count(*) AS judged_turns,
         count(*) FILTER (WHERE x.reaction = 'corrected') AS corrected_turns
    FROM (SELECT DISTINCT l.project_uuid, l.folder, t.id, t.reaction
            FROM loads l JOIN turns t ON t.id = l.operator_turn_id) x
   GROUP BY x.project_uuid, x.folder),
base AS (
  SELECT w.project_uuid, w.folder, count(t.id) AS base_judged_turns,
         count(t.id) FILTER (WHERE t.reaction = 'corrected') AS base_corrected_turns
    FROM win w LEFT JOIN turns t ON t.project_id = w.project_id AND t.created_at >= w.window_start
   GROUP BY w.project_uuid, w.folder)
SELECT w.project_uuid, w.folder, w.window_start, w.window_end,
       coalesce(l.loads, 0) AS loads, coalesce(l.load_sessions, 0) AS load_sessions,
       coalesce(l.loaded_turns, 0) AS loaded_turns, coalesce(j.judged_turns, 0) AS judged_turns,
       coalesce(j.corrected_turns, 0) AS corrected_turns,
       b.base_judged_turns, b.base_corrected_turns
  FROM win w
  LEFT JOIN loaded l ON l.project_uuid = w.project_uuid AND l.folder = w.folder
  LEFT JOIN judged j ON j.project_uuid = w.project_uuid AND j.folder = w.folder
  JOIN base b ON b.project_uuid = w.project_uuid AND b.folder = w.folder"""


def shipped_skills(ledger, keys):
    """The (project uuid, folder, first shipped day) of each key the promoter still owns, skipping any a query must not carry."""
    found = []
    for key in keys:
        project, _, folder = key.partition(':')
        day = ledger[key].get('first_emitted', '')
        if UUID.fullmatch(project) and FOLDER.fullmatch(folder) and DAY.fullmatch(day):
            found.append((project, folder, day))
    return found


def live_query(shipped):
    values = ','.join(f"('{project}'::uuid,'{folder}','{day}'::date)" for project, folder, day in shipped)
    return LIVE_SQL.format(values=values, days=WINDOW_DAYS)


def measure(query, ledger, keys):
    """Live numbers per ledger key, from one read-only query over every shipped skill."""
    shipped = shipped_skills(ledger, keys)
    if not shipped:
        return {}
    days = {(project, folder): day for project, folder, day in shipped}
    return {f"{row['project_uuid']}:{row['folder']}": {**row, 'first_shipped_on': days[(row['project_uuid'], row['folder'])]}
            for row in query(live_query(shipped))}


def corrected_above_base(live):
    """At least 10 loads and a corrected rate above the project's, compared without division."""
    judged, base = live['judged_turns'], live['base_judged_turns']
    return (live['loads'] >= MIN_LOADS_TO_RETIRE and judged > 0 and base > 0
            and live['corrected_turns'] * base > live['base_corrected_turns'] * judged)


def retire_corrected_above_base(desired, ledger, live, today, emitted):
    retired = sorted(key for key in emitted
                     if not ledger[key]['retired_at'] and key in live and corrected_above_base(live[key]))
    for key in retired:
        desired.pop(str(emitted[key]), None)
        mark_retired(ledger[key], today, 'corrected_above_base')
    return retired


def mark_retired(entry, today, reason):
    entry['retired_at'] = today.isoformat()
    entry['retired_reason'] = reason
    entry['retired_ts'] = datetime.datetime.now(datetime.timezone.utc).isoformat()


def retired_instant(value):
    if not value:
        return None
    moment = datetime.datetime.fromisoformat(value)
    return (moment if moment.tzinfo else moment.replace(tzinfo=datetime.timezone.utc)).isoformat()


def outcome(key, live, entry):
    project, _, folder = key.partition(':')
    retired = entry.get('retired_ts') or entry.get('retired_at')
    return {'project': project, 'folder': folder, 'first_shipped_on': live['first_shipped_on'],
            'window_start': live['window_start'], 'window_end': live['window_end'],
            'loads': live['loads'], 'load_sessions': live['load_sessions'], 'loaded_turns': live['loaded_turns'],
            'judged_turns': live['judged_turns'], 'corrected_turns': live['corrected_turns'],
            'base_judged_turns': live['base_judged_turns'], 'base_corrected_turns': live['base_corrected_turns'],
            'retired_at': retired_instant(retired),
            'retired_reason': entry.get('retired_reason') if retired else None}


def record(ledger, live):
    """Keep each measured skill's latest numbers in the ledger, unpublished until the proxy has stored them."""
    for key, numbers in live.items():
        entry = ledger[key]
        entry['outcome'] = outcome(key, numbers, entry)
        entry['published'] = False


def unpublished(ledger):
    return {key: entry['outcome'] for key, entry in ledger.items() if entry.get('outcome') and not entry.get('published')}


def publish(client, ledger):
    """Send every outcome the proxy has not stored yet; a failure leaves them for the next run."""
    waiting = unpublished(ledger)
    if not waiting:
        return {'sent': 0, 'error': None}
    try:
        client.request(OUTCOMES_PATH, {'outcomes': list(waiting.values())})
    except RuntimeError as error:
        return {'sent': 0, 'error': str(error)}
    for key in waiting:
        ledger[key]['published'] = True
    return {'sent': len(waiting), 'error': None}
