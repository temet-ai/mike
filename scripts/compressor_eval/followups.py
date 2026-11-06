"""Observed follow-ups, isolated by session and agent with bounded chronology."""
import collections
from datetime import datetime


def stamp(value):
    return datetime.fromisoformat(value.replace('Z', '+00:00')).timestamp()


def lane(row):
    if row.get('session_id') is None:
        return None
    identity = (row.get('agent_instance_id'), row.get('claude_code_agent_id'))
    if not any(identity) and row.get('origin') not in ('primary', 'primary_first'):
        return None
    return (row['session_id'], *identity)


def analyze(cuts, arms, calls, sources, parse_cut, hidden_text, anchors, file_type, until, window=1800):
    by_file = collections.defaultdict(list)
    for call in calls:
        identity = lane(call)
        if identity is not None:
            by_file[(identity, call['file_path'])].append(call)
    table = collections.defaultdict(collections.Counter)
    seen, reads, edits = set(), collections.defaultdict(set), collections.defaultdict(set)
    gaps = collections.Counter()
    for cut in cuts:
        identity = lane(cut)
        key = (identity, cut['wire_id'])
        if identity is None:
            gaps['missing_agent_identity'] += 1
            continue
        if key in seen:
            continue
        seen.add(key)
        parsed = parse_cut(cut)
        arm = arms.get(cut['session_id'])
        source = sources.get(key)
        if not parsed or arm is None or source is None:
            gaps['missing_source_or_policy'] += 1
            continue
        path, kept = parsed
        if source['file_path'] != path:
            gaps['source_path_mismatch'] += 1
            continue
        hidden, shown = hidden_text(source['content'], kept)
        if hidden is None:
            gaps['missing_result_text'] += 1
            continue
        after = max(stamp(cut['created_at']), stamp(source['result_at']))
        if after + window > until:
            gaps['right_censored'] += 1
            continue
        bucket = (file_type(path), arm)
        row = table[bucket]
        row['cuts'] += 1
        later = [c for c in by_file[(identity, path)]
                 if c['id'] != source['id'] and after < stamp(c['created_at']) <= after + window]
        matching_reads = {c['id'] for c in later if c['tool_name'] == 'Read'}
        matching_edits = {c['id'] for c in later if c['tool_name'] in ('Edit', 'MultiEdit')
                          and any(a and a in hidden and a not in shown for a in anchors(c))}
        reads[bucket].update(matching_reads)
        edits[bucket].update(matching_edits)
        row['exposures_with_hidden_edit'] += bool(matching_edits)
        row['exposures_with_reread'] += bool(matching_reads)
        row['reread'] = len(reads[bucket])
        row['edit_hidden'] = len(edits[bucket])
    return table, dict(gaps)
