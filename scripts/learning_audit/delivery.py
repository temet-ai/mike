import json
import urllib.error
import urllib.parse
import urllib.request

INSPECT = 'http://127.0.0.1:9989'
ARMS = ('mod', 'control')


def fetch(start, end, base=INSPECT):
    """Read the mod and control arm comparison from the running proxy; None when its build lacks the route."""
    url = base + '/api/lessons/delivery-comparison?' + urllib.parse.urlencode({'from': start, 'until': end})
    try:
        with urllib.request.urlopen(url, timeout=120) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def rate(value, digits, share=False):
    if value is None:
        return 'no data'
    return f'{value:.1%}' if share else f'{value:.{digits}f}'


def row(arm):
    total = arm['input_tokens'] + arm['cache_read_tokens'] + arm['cache_write_tokens']
    return (f"| {arm['arm']} | {arm['sessions']} | {arm['operator_turns']} "
            f"| {rate(arm['lessons_per_turn'], 2)} ({arm['lessons_delivered']} lessons / {arm['operator_turns']} turns) "
            f"| {rate(arm['cache_read_share'], 1, share=True)} ({arm['cache_read_tokens']} / {total} input tokens) "
            f"| {rate(arm['corrected_rate'], 1, share=True)} ({arm['corrected_turns']} corrected / {arm['judged_turns']} judged) |")


def render(comparison):
    heading = ['', '## Lesson delivery: mike mod against proxy injection', '']
    if comparison is None:
        return heading + ['Not available: the running proxy does not have the delivery comparison route yet. '
                          'Deploy the build that adds `GET /api/lessons/delivery-comparison`.']
    arms = {arm['arm']: arm for arm in comparison['arms']}
    intro = ('While the `mod_lesson_delivery` setting is on, one session in five stays on proxy injection (the control '
             'arm) and the rest get their lessons from the mike mod as hook context (the mod arm). A session is placed '
             'in an arm by a hash of its id, so the arms are comparable but not matched by project or task. '
             'A rate over a few turns is a reason to read the sessions, not a result.')
    if not any(arms.get(name, {}).get('sessions') for name in ARMS):
        return heading + [intro, '', 'No session in either arm made a request in this window. The setting was off, '
                          'or no session has the mike mod installed.']
    lines = heading + [intro, '',
                       '| Arm | Sessions | Operator turns | Lessons per turn | Cache read share of input tokens | Judge corrected rate |',
                       '|---|---:|---:|---|---|---|']
    lines += [row(arms[name]) for name in ARMS if name in arms]
    for name in ARMS:
        if name in arms:
            lines.append(f"- {name}: {arms[name]['usage_requests']} of {arms[name]['requests']} requests carried token usage.")
    return lines
