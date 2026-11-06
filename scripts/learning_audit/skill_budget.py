"""Limits on the skills mike writes into a project: one per purpose, and a cap per project."""
import datetime

CAP_SETTING = 'lessons.skills.max_per_project'
DEFAULT_CAP = 12
MIN_CAP, MAX_CAP = 1, 100
YOUNG_DAYS = 7
MIN_SHARED_WORDS = 2
FILLER = frozenset({'the', 'a', 'an', 'of', 'and', 'for', 'to', 'in', 'on', 'with'})


def stem(word):
    if len(word) > 5 and word.endswith('ing'):
        return word[:-3]
    if len(word) > 4 and word.endswith('ed'):
        return word[:-2]
    if len(word) > 3 and word.endswith('s') and not word.endswith('ss'):
        return word[:-1]
    return word


def purpose_words(topic):
    words = (word for word in ''.join(c if c.isascii() and c.isalnum() else ' ' for c in topic.lower()).split()
             if word not in FILLER)
    return {stem(word) for word in words}


def same_purpose(first, second):
    """Two topics share a purpose when they use the same words, or share at least two and three quarters of the shorter one's."""
    a, b = purpose_words(first), purpose_words(second)
    if not a or not b:
        return False
    if a == b:
        return True
    shared = len(a & b)
    return shared >= MIN_SHARED_WORDS and shared * 4 >= min(len(a), len(b)) * 3


def project_of(key):
    return key.split(':', 1)[0]


def read_cap(query):
    rows = query(f"SELECT value FROM config.settings WHERE key = '{CAP_SETTING}'")
    value = rows[0]['value'] if rows else None
    if isinstance(value, bool) or not isinstance(value, int) or not MIN_CAP <= value <= MAX_CAP:
        return DEFAULT_CAP
    return value


def hold_overlapping(candidates, shipped):
    """Keep one skill per purpose in each project. candidates maps key -> (topic, lesson count);
    returns held key -> the key that keeps the purpose. A skill already shipped wins, then the one with more lessons."""
    by_project = {}
    for key in candidates:
        by_project.setdefault(project_of(key), []).append(key)
    held = {}
    for keys in by_project.values():
        survivors = []
        for key in sorted(keys, key=lambda k: (k not in shipped, -candidates[k][1], k)):
            match = next((s for s in survivors if same_purpose(candidates[s][0], candidates[key][0])), None)
            if match is None:
                survivors.append(key)
            else:
                held[key] = match
    return held


def age_days(entry, today):
    return (today - datetime.date.fromisoformat(entry['first_emitted'])).days


def enforce_cap(ledger, loads, today, live, fresh, lessons, cap, reviewed=frozenset()):
    """Return (retire, hold). When a project has more live skills than cap, a generated skill (one with no
    reviewed draft) gives way before a reviewed one, whatever its age. Among the rest, the skill with the least
    evidence of use (fewest loads in the window, then the oldest) is retired first, and a reviewed skill younger
    than a week is never retired. New skills are held back only for the overflow that retiring cannot absorb,
    generated ones before reviewed ones."""
    by_project = {}
    for key in live:
        by_project.setdefault(project_of(key), []).append(key)
    retire, hold = [], []
    for keys in by_project.values():
        overflow = len(keys) - cap
        if overflow <= 0:
            continue
        reviewed_newcomers = sum(1 for k in keys if k in fresh and k in reviewed)
        generated = sorted((k for k in keys if k not in fresh and k not in reviewed),
                           key=lambda k: (loads.get(k, 0), ledger[k]['first_emitted'], k))
        mature = sorted((k for k in keys if k not in fresh and k in reviewed and age_days(ledger[k], today) >= YOUNG_DAYS),
                        key=lambda k: (loads.get(k, 0), ledger[k]['first_emitted'], k))
        displaced = generated[:min(overflow, reviewed_newcomers)]
        rest = overflow - len(displaced)
        aged = [k for k in generated[len(displaced):] if age_days(ledger[k], today) >= YOUNG_DAYS] + mature
        retire += displaced + aged[:rest]
        remaining = rest - len(aged[:rest])
        newcomers = sorted((k for k in keys if k in fresh), key=lambda k: (k in reviewed, lessons.get(k, 0), k))
        hold += newcomers[:remaining]
    return retire, hold
