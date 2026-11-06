"""Conservative shell hints. Never execute captured commands or resolve shell code."""
import posixpath
import re
import shlex

OPERATIONS = {'grep': 'filter', 'rg': 'filter', 'jq': 'projection',
              'awk': 'projection', 'sed': 'slice', 'head': 'slice',
              'cut': 'projection', 'tail': 'tail_or_poll', 'cat': 'read'}


def inspect(call):
    inputs = call.get('input') or {}
    if not isinstance(inputs, dict):
        return set(), {'unknown'}
    if call['tool_name'] == 'Read':
        path = inputs.get('file_path', '')
        paths = {path} if path.startswith('/') else set()
        return paths, {'bounded_read' if 'offset' in inputs or 'limit' in inputs else 'read'}
    if call['tool_name'] == 'TaskOutput':
        return set(), {'poll'}
    command = inputs.get('command', '')
    if not command:
        return set(), {'unknown'}
    # A full shell parser is deliberately not approximated for embedded programs.
    if any(token in command for token in ('<<', '$(', '`')):
        return set(), {'unparsed_shell'}
    try:
        lexer = shlex.shlex(command, posix=True, punctuation_chars=';&|<>\n')
        lexer.whitespace = ' \t\r'
        lexer.whitespace_split = True
        words = list(lexer)
    except ValueError:
        return set(), {'unparsed_shell'}
    variables, segments, segment = {}, [], []
    for word in words + [';']:
        if word and all(c in ';&|<>\n' for c in word):
            if word in ('>', '>>', '<'):
                return set(), {'redirect_or_mixed'}
            if segment:
                segments.append(segment)
                segment = []
        else:
            segment.append(word)
    paths, kinds = set(), set()
    for segment in segments:
        while segment and re.match(r'^[A-Za-z_][A-Za-z0-9_]*=', segment[0]):
            name, value = segment.pop(0).split('=', 1)
            variables[name] = value
        if not segment:
            continue
        executable = posixpath.basename(segment[0])
        kind = OPERATIONS.get(executable)
        if kind is None:
            kinds.add('other_or_mixed')
            continue
        kinds.add(kind)
        for word in segment[1:]:
            if re.fullmatch(r'\$[A-Za-z_][A-Za-z0-9_]*', word):
                word = variables.get(word[1:], '')
            if word.startswith('/') and not any(c in word for c in '*?[$\n'):
                paths.add(posixpath.normpath(word))
    return paths, kinds or {'unknown'}
