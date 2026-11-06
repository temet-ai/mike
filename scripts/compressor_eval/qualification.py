"""Explicit task and recovery evidence required before widening compression."""
import hashlib
import json
from datetime import datetime, timezone

from .trials import compare


def policy_hash(policy):
    return hashlib.sha256(json.dumps(policy, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def qualifies(evidence, policy, now=None):
    if not isinstance(evidence, dict) or evidence.get('schema') != 'compressor-qualification-v1':
        return False
    if evidence.get('policy_sha256') != policy_hash(policy):
        return False
    if policy and policy.get('implementation') and not policy.get('build_sha'):
        return False
    try:
        at = datetime.fromisoformat(evidence['completed_at'].replace('Z', '+00:00'))
        age = ((now or datetime.now(timezone.utc)) - at).total_seconds()
        if not 0 <= age <= 7 * 86400:
            return False
        recovery = evidence['recovery']
        if recovery.get('passed') is not True or recovery.get('weakened_control_failed') is not True or not recovery.get('evidence_ref'):
            return False
        pairs = evidence['pairs']
        scored = compare(pairs)
        if not scored or not all(r['both_correct'] for r in scored):
            return False
        if any(r[arm]['actual_total_tokens'] is None or r[arm]['missing_results']
               for r in scored for arm in ('original', 'compact')):
            return False
        for family in ('documentation', 'configuration', 'test-output'):
            selected = [p for p in pairs if p.get('family') == family]
            if len({p['task_id'] for p in selected}) < 3:
                return False
        for pair in pairs:
            for arm in ('original', 'compact'):
                run = pair[arm]
                if not run['calls'] or not run.get('model_turns') or not run.get('model_config_hash'):
                    return False
                if not run.get('activation_ref') or not run.get('constraint_verdict_ref'):
                    return False
        return True
    except (KeyError, TypeError, ValueError):
        return False
