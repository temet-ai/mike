"""Supported feature API isolation for a fresh owned reliability instance only.

TOML flags are insufficient: startup reconciliation replaces them with database
operator overrides or compiled feature defaults. Persist every disable through
Mike's API, restart the owned process for OnRestart keys, then verify the live
feature view before admitting synthetic capture traffic. No direct schema writes.
"""
import json

from journey_process import JourneyError, require
from journey_wire import exchange

COMPILED_EMBEDDING_URL = "http://127.0.0.1:7997"

FEATURES = {
    "agent_lessons": ("session", "new_sessions"),
    "change_record": ("global", "immediate"),
    "change_record_tail": ("global", "immediate"),
    "channel_link_sweep": ("global", "live_disable"),
    "client_serves_agent_lessons": ("global", "immediate"),
    "client_serves_change_record_tail": ("global", "immediate"),
    "client_serves_git_read_hint": ("global", "immediate"),
    "client_serves_mail": ("global", "immediate"),
    "client_serves_memory_keeper": ("global", "immediate"),
    "client_serves_prompt_shaping": ("global", "immediate"),
    "client_serves_proxy_tools": ("global", "immediate"),
    "client_serves_recall": ("global", "immediate"),
    "client_serves_reminders": ("global", "immediate"),
    "client_serves_self_identity": ("global", "immediate"),
    "client_serves_weasel_guard": ("global", "immediate"),
    "client_serves_workspace_briefing": ("global", "immediate"),
    "context_focus_question": ("session", "new_sessions"),
    "db_resync": ("global", "on_restart"),
    "directive_proposer": ("global", "live_disable"),
    "dossier_builder": ("global", "immediate"),
    "file_read_compression": ("session", "new_sessions"),
    "fleet_auto_start": ("global", "immediate"),
    "harvester": ("global", "immediate"),
    "judge_lessons": ("global", "live_disable"),
    "judge_worker": ("global", "live_disable"),
    "launch_brief": ("global", "immediate"),
    "lesson_worker": ("global", "live_disable"),
    "mailbox_redelivery": ("session", "new_sessions"),
    "memory_keeper": ("global", "live_disable"),
    "mod_lesson_delivery": ("global", "immediate"),
    "native_peer_mail": ("global", "immediate"),
    "prompt_assistant": ("global", "immediate"),
    "proxy_tools": ("session", "new_sessions"),
    "recall_injector": ("session", "new_sessions"),
    "reminder_on_token": ("session", "new_sessions"),
    "reminders": ("session", "new_sessions"),
    "repeated_notices": ("session", "new_sessions"),
    "retention_sweep": ("global", "live_disable"),
    "seat_memory_index": ("global", "live_disable"),
    "semantic_context": ("global", "immediate"),
    "self_identity_reminder": ("session", "new_sessions"),
    "session_end_shadow": ("global", "live_disable"),
    "session_judge": ("global", "live_disable"),
    "shell_output_compression": ("session", "new_sessions"),
    "steerer": ("global", "live_disable"),
    "turn_guard": ("global", "live_disable"),
    "weasel_intercept": ("session", "new_sessions"),
    "workspace_briefing": ("session", "new_sessions"),
    "workspace_capture_registration": ("session", "new_sessions"),
    "wright": ("global", "live_disable"),
}


def feature_request(port, budget, key=None):
    # Only exact, source-reviewed feature names can become mutation endpoints.
    require(key is None or key in FEATURES, "RELIABILITY_FEATURE_KEY")
    path = "/api/settings/features" + ("/" + key if key is not None else "")
    method = "GET" if key is None else "PATCH"
    body = b"" if key is None else b'{"enabled":false}'
    headers = (f"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n"
               "User-Agent: mike-archive-reliability\r\n"
               "X-Change-Reason: Isolate newly allocated synthetic reliability fixture\r\n"
               "Connection: close\r\n")
    if body:
        headers += f"Content-Type: application/json\r\nContent-Length: {len(body)}\r\n"
    status, _, raw = exchange(port, headers.encode() + b"\r\n" + body, timeout=min(5, budget.remaining()))
    require(status == 200, "RELIABILITY_FEATURE_API_STATUS")
    budget.now()
    try:
        return json.loads(raw)
    except (ValueError, UnicodeError) as error:
        raise JourneyError("RELIABILITY_FEATURE_API_JSON") from error


def feature_row(row):
    require(isinstance(row, dict) and set(row) == {"key", "enabled", "scope", "applies"},
            "RELIABILITY_FEATURE_SCHEMA")
    key = row["key"]
    require(isinstance(key, str) and key in FEATURES and type(row["enabled"]) is bool
            and (row["scope"], row["applies"]) == FEATURES[key], "RELIABILITY_FEATURE_SCHEMA")
    return key, row["enabled"]


def read_features(port, budget):
    value = feature_request(port, budget)
    require(isinstance(value, dict) and set(value) == {"features"} and isinstance(value["features"], list)
            and len(value["features"]) == len(FEATURES), "RELIABILITY_FEATURE_SET")
    flags = {}
    for row in value["features"]:
        key, enabled = feature_row(row)
        require(key not in flags, "RELIABILITY_FEATURE_SET")
        flags[key] = enabled
    require(set(flags) == set(FEATURES), "RELIABILITY_FEATURE_SET")
    return flags


def verify_features(port, budget):
    flags = read_features(port, budget)
    require(not any(flags.values()), "RELIABILITY_FEATURE_STILL_ENABLED")
    return {"effective_feature_flags": flags, "all_expected_flags_disabled": True}


def disable_features(port, budget):
    before = read_features(port, budget)
    # Persist even an already-false default so startup cannot silently replace it.
    for key in FEATURES:
        actual_key, enabled = feature_row(feature_request(port, budget, key))
        require(actual_key == key and enabled is False, "RELIABILITY_FEATURE_DISABLE_REJECTED")
    observation = verify_features(port, budget)
    return dict(observation, requested_disabled_keys=list(FEATURES),
                initially_enabled_count=sum(before.values()), supported_api_only=True,
                restart_required_for_on_restart_flags=True)


def verify_service_prerequisites(health):
    """Check observed disabled-service prerequisites, never network isolation.

    config/load/mod.rs resets embedding configuration after TOML/env loading.
    health/probe.rs returns not_configured before HTTP only when it is disabled.
    The compiled endpoint is not owned by this fixture and must remain unused.
    """
    require(isinstance(health, dict)
            and isinstance(health.get("embedding"), dict)
            and health["embedding"].get("reachable_state") == "not_configured"
            and health["embedding"].get("reachable") is False
            and health["embedding"].get("url") == COMPILED_EMBEDDING_URL
            and isinstance(health.get("llm"), dict)
            and health["llm"].get("key_configured") is False, "RELIABILITY_SERVICE_PREREQUISITES")
    return {"embedding_disabled": True, "embedding_url_is_compiled_default": True,
            "embedding_endpoint_owned": False, "openrouter_key_absent": True,
            "all_transport_isolation_verified": False}
