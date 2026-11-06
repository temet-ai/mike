"""results.json and report.md from the graded cells: one row per cell, medians per arm over
the cells whose modality fired, and the channels that stayed open."""
import statistics

COLUMNS = ('accepted', 'landed', 'wall_seconds', 'sessions', 'requests', 'tool_calls',
           'fresh', 'cache_write', 'cache_read', 'output', 'weighted')
OPEN_CHANNELS = (
    'The prompt cache is shared across arms: cache_read and cache_write are shown apart and the '
    'cell order was shuffled, nothing more.',
    "The fleet arm's seats run with the operator's user-scope Claude Code configuration "
    '(settings.json, CLAUDE.md, MCP servers); the claude -p arms run with an empty home.',
    'Fleet leaf calls (judge, checker, problem solver) go to mike\'s own provider and are not '
    'in the fleet row\'s token columns unless the run record carries them.',
)


def row_of(cell_record):
    row = {'cell': cell_record['id'], 'arm': cell_record['arm'], 'rep': cell_record['rep'],
           'stop': cell_record.get('stop', '?'), 'fidelity': cell_record.get('fidelity', {}).get('ok'),
           'wall_seconds': cell_record.get('wall_seconds')}
    grade = cell_record.get('grade') or {}
    row['accepted'] = grade.get('accepted')
    row['landed'] = grade.get('landed')
    row['of'] = grade.get('of')
    row['in_tree_accepted'] = (grade.get('in_tree') or {}).get('accepted')
    for key, value in (cell_record.get('totals') or {}).items():
        row[key] = value
    return row


def arm_summary(rows, arm):
    mine = [r for r in rows if r['arm'] == arm]
    faithful = [r for r in mine if r.get('fidelity') is True]
    summary = {'cells': len(mine), 'faithful': len(faithful), 'medians': {}, 'ranges': {}}
    if len(faithful) < 3:
        summary['note'] = 'fewer than three faithful cells: rows only, no medians'
        return summary
    for key in COLUMNS:
        values = [r[key] for r in faithful if isinstance(r.get(key), (int, float))]
        if values:
            summary['medians'][key] = statistics.median(values)
            summary['ranges'][key] = [min(values), max(values)]
    return summary


def build_results(plan, cell_records):
    rows = [row_of(c) for c in sorted(cell_records, key=lambda c: (c['arm'], c['rep']))]
    return {'schema': 'modality-bench-results-v1', 'plan': {k: plan[k] for k in ('model', 'effort', 'reps', 'seed', 'arms', 'cap_seconds')},
            'environment': plan.get('environment', {}), 'modality_lines': plan.get('modality_lines', {}),
            'rows': rows, 'arms': {arm: arm_summary(rows, arm) for arm in plan['arms']},
            'open_channels': list(OPEN_CHANNELS)}


def fmt(value):
    if value is None:
        return '-'
    if isinstance(value, float):
        return f'{value:.0f}' if value >= 100 else f'{value:.1f}'
    return str(value)


def table(headers, rows):
    lines = ['| ' + ' | '.join(headers) + ' |', '|' + '---|' * len(headers)]
    lines += ['| ' + ' | '.join(fmt(r.get(h)) for h in headers) + ' |' for r in rows]
    return '\n'.join(lines)


def render_report(results):
    plan = results['plan']
    out = ['# Modality bench', '',
           f"Model `{plan['model']}`, effort `{plan['effort']}`, {plan['reps']} rep(s) per arm, seed {plan['seed']}, "
           f"cap {plan['cap_seconds']} s. Client {results['environment'].get('client_version', '?')}.", '',
           '## Cells', '',
           table(('cell', 'stop', 'fidelity', 'accepted', 'landed', 'in_tree_accepted', 'wall_seconds', 'sessions',
                  'requests', 'tool_calls', 'fresh', 'cache_write', 'cache_read', 'output', 'weighted'), results['rows']),
           '', '## Per arm (median over faithful cells; range in brackets)', '']
    summary_rows = []
    for arm, summary in results['arms'].items():
        row = {'arm': arm, 'cells': summary['cells'], 'faithful': summary['faithful']}
        for key in COLUMNS:
            if key in summary['medians']:
                low, high = summary['ranges'][key]
                row[key] = f"{fmt(summary['medians'][key])} [{fmt(low)}..{fmt(high)}]"
            else:
                row[key] = summary.get('note', '-') if key == 'accepted' else '-'
        summary_rows.append(row)
    out += [table(('arm',) + ('cells', 'faithful') + COLUMNS, summary_rows), '', '## Modality lines', '']
    out += [f'- `{arm}`: {line or "(the definition is the split)"}' for arm, line in results['modality_lines'].items()]
    out += ['', '## Channels that stayed open', ''] + [f'- {c}' for c in results['open_channels']]
    out += ['', 'Weighted = fresh x1.0 + cache_write x1.25 + cache_read x0.1 + output x5.0: a relative cost, not dollars. '
            'A difference smaller than the range is no difference.', '']
    return '\n'.join(out)
