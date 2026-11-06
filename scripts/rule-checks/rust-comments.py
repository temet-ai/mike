#!/usr/bin/env python3
import re
import sys

STRING = re.compile(r'b?r(#*)".*?"\1|b?"(?:[^"\\]|\\.)*"|\'(?:[^\'\\]|\\.)\'')
COMMENT = re.compile(r"//|/\*|^\s*\*(\s|/|$)")
GRADER_ALLOW = re.compile(r"grader-allow:")


def has_comment(text):
    if GRADER_ALLOW.search(text):
        return False
    return bool(COMMENT.search(STRING.sub('""', text)))


def parse(diff):
    added = []
    removed = set()
    path = None
    line_no = 0
    for raw in diff.splitlines():
        if raw.startswith("+++ "):
            target = raw[4:]
            path = None if target == "/dev/null" else re.sub(r"^[a-z]/", "", target)
        elif raw.startswith("--- "):
            continue
        elif raw.startswith("@@"):
            match = re.search(r"\+(\d+)", raw)
            line_no = int(match.group(1)) if match else 0
        elif raw.startswith("-"):
            removed.add(raw[1:].strip())
        elif raw.startswith("+"):
            if path and path.endswith(".rs"):
                added.append((path, line_no, raw[1:]))
            line_no += 1
        elif not raw.startswith("\\"):
            line_no += 1
    return added, removed


def main():
    added, removed = parse(sys.stdin.read())
    findings = [
        f"{path}:{no}: {text.strip()}"
        for path, no, text in added
        if has_comment(text) and text.strip() not in removed
    ]
    if findings:
        print("\n".join(findings))
        return 1
    print("CLEAN")
    return 0


if __name__ == "__main__":
    sys.exit(main())
