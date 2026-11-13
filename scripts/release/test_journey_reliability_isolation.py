"""Offline effective-feature isolation contracts; no runtime API or DB access."""
import copy
import json
from pathlib import Path
import re
from unittest.mock import patch
import unittest

import journey_reliability as reliability
import journey_reliability_isolation as isolation
from journey_process import JourneyError
from test_journey_reliability import OfflineTests


def row(key, enabled=False):
    scope, applies = isolation.FEATURES[key]
    return {"key": key, "enabled": enabled, "scope": scope, "applies": applies}


def features(enabled=()):
    enabled = set(enabled)
    return {"features": [row(key, key in enabled) for key in isolation.FEATURES]}



class SourceRoots:
    """crate mike's sources across the crates split from it (docs/adr/0002):
    a module keeps its path under whichever crate's src/ now holds it."""

    def __init__(self):
        crates = Path(__file__).resolve().parents[2] / "crates"
        self.roots = [crates / name / "src" for name in ("mike", "mike-base", "mike-features")]

    def file(self, relative):
        for root in self.roots:
            if (root / relative).exists():
                return root / relative
        return self.roots[0] / relative

class EffectiveIsolationTests(OfflineTests):
    def test_supported_api_disables_reconciled_defaults_before_owned_restart(self):
        # A false TOML flag did not win: startup reconciled these DB/code defaults.
        values = {key: key in {"lesson_worker", "harvester"} for key in isolation.FEATURES}
        patched = []
        def request(_port, _budget, key=None):
            if key is None:
                return features(key for key, enabled in values.items() if enabled)
            patched.append(key)
            values[key] = False
            return row(key)
        with patch.object(isolation, "feature_request", side_effect=request):
            observation = isolation.disable_features(10002, reliability.Budget(1))
        self.assertEqual(set(patched), set(isolation.FEATURES))
        self.assertEqual(len(patched), len(isolation.FEATURES))
        self.assertEqual(observation["initially_enabled_count"], 2)
        self.assertTrue(observation["restart_required_for_on_restart_flags"])
        self.assertTrue(observation["supported_api_only"])
        self.assertTrue(all(value is False for value in observation["effective_feature_flags"].values()))

    def test_missing_unknown_duplicate_or_changed_metadata_flags_fail_closed(self):
        invalid = []
        value = features(); value["features"].pop(); invalid.append(value)
        value = features(); value["features"][0]["key"] = "unreviewed_feature"; invalid.append(value)
        value = features(); value["features"][0] = value["features"][1]; invalid.append(value)
        value = features(); value["features"][0]["enabled"] = 0; invalid.append(value)
        value = features(); value["features"][0]["applies"] = "immediate"; invalid.append(value)
        for value in invalid:
            with self.subTest(value=value), patch.object(isolation, "feature_request", return_value=value), self.assertRaises(JourneyError):
                isolation.read_features(10002, reliability.Budget(1))

    def test_effective_enabled_flag_never_passes_just_because_disable_was_requested(self):
        with patch.object(isolation, "feature_request", return_value=features(["harvester"])):
            with self.assertRaisesRegex(JourneyError, "FEATURE_STILL_ENABLED"):
                isolation.verify_features(10002, reliability.Budget(1))
        with patch.object(isolation, "feature_request", side_effect=[features(), row("agent_lessons", True)]):
            with self.assertRaisesRegex(JourneyError, "FEATURE_DISABLE_REJECTED"):
                isolation.disable_features(10002, reliability.Budget(1))

    def test_mutation_targets_only_reviewed_feature_key_with_fixed_false_payload(self):
        response = json.dumps(row("harvester")).encode()
        with patch.object(isolation, "exchange", return_value=(200, {}, response)) as transport:
            self.assertEqual(isolation.feature_request(10002, reliability.Budget(1), "harvester"), row("harvester"))
        port, raw = transport.call_args.args
        self.assertEqual(port, 10002)
        self.assertTrue(raw.startswith(b"PATCH /api/settings/features/harvester HTTP/1.1\r\n"))
        self.assertTrue(raw.endswith(b'\r\n\r\n{"enabled":false}'))
        self.assertIn(b"Host: 127.0.0.1:10002\r\n", raw)
        with patch.object(isolation, "exchange") as transport, self.assertRaisesRegex(JourneyError, "FEATURE_KEY"):
            isolation.feature_request(10002, reliability.Budget(1), "harvester/../other")
        transport.assert_not_called()

    def test_nonzero_status_or_malformed_json_is_sanitized(self):
        for response in ((403, {}, b"private diagnostics"), (200, {}, b"private diagnostics")):
            with patch.object(isolation, "exchange", return_value=response), self.assertRaises(JourneyError) as error:
                isolation.feature_request(10002, reliability.Budget(1))
            self.assertNotIn("private", str(error.exception))

    def test_disabled_embedding_compiled_url_and_absent_key_are_service_prerequisites(self):
        valid = {"embedding": {"reachable_state": "not_configured", "reachable": False,
                               "url": "http://127.0.0.1:7997"},
                 "llm": {"key_configured": False}}
        before = copy.deepcopy(valid)
        observed = isolation.verify_service_prerequisites(valid)
        self.assertTrue(observed["embedding_disabled"])
        self.assertTrue(observed["embedding_url_is_compiled_default"])
        self.assertTrue(observed["openrouter_key_absent"])
        self.assertFalse(observed["embedding_endpoint_owned"])
        self.assertFalse(observed["all_transport_isolation_verified"])
        self.assertEqual(valid, before)
        for group, field, value in (("embedding", "reachable_state", "ok"),
                                    ("embedding", "reachable_state", "failed"),
                                    ("embedding", "reachable_state", "timed_out"),
                                    ("embedding", "reachable", True),
                                    ("embedding", "reachable", 0),
                                    ("embedding", "url", "http://127.0.0.1:10004"),
                                    ("embedding", "url", "https://external.invalid"),
                                    ("llm", "key_configured", True),
                                    ("llm", "key_configured", 0)):
            changed = copy.deepcopy(valid)
            changed[group][field] = value
            with self.subTest(group=group, field=field, value=value), self.assertRaisesRegex(JourneyError, "SERVICE_PREREQUISITES"):
                isolation.verify_service_prerequisites(changed)
        for group in ("embedding", "llm"):
            for field in valid[group]:
                changed = copy.deepcopy(valid)
                del changed[group][field]
                with self.subTest(missing=(group, field)), self.assertRaisesRegex(JourneyError, "SERVICE_PREREQUISITES"):
                    isolation.verify_service_prerequisites(changed)
        for changed in (None, {}, {"embedding": [], "llm": {}}, {"embedding": valid["embedding"], "llm": None}):
            with self.subTest(malformed=changed), self.assertRaisesRegex(JourneyError, "SERVICE_PREREQUISITES"):
                isolation.verify_service_prerequisites(changed)

    def test_embedding_prerequisite_facts_match_loader_and_health_source(self):
        root = SourceRoots()
        load = root.file("config/load/mod.rs").read_text()
        reset = load.index("config.embedding = EmbeddingConfig::default();")
        self.assertLess(load.index("figment.extract()"), reset)
        self.assertLess(reset, load.index("config.embedding.enabled = v;"))
        source = root.file("analysis/embeddings/mod.rs").read_text()
        self.assertIn(f'const DEFAULT_INFINITY_URL: &str = "{isolation.COMPILED_EMBEDDING_URL}";', source)
        block = source.split("impl Default for EmbeddingConfig", 1)[1]
        self.assertRegex(block, r"enabled:\s*false")
        self.assertIn("url: DEFAULT_INFINITY_URL.to_string()", block)
        # the crate split widened pub(crate) to pub on items other crates reach
        probe_source = root.file("api/handlers/health/probe.rs").read_text().replace("pub(crate) ", "pub ")
        probe = probe_source.split("pub async fn probe_embedding(", 1)[1].split(
            "pub async fn probe_persistence", 1)[0]
        self.assertRegex(probe, r"if !embedding_service\.is_available\(\)\s*\{\s*return ProbeState::NotConfigured;")
        self.assertLess(probe.index("return ProbeState::NotConfigured;"), probe.index("probe_embedding_url("))
        health = root.file("api/handlers/health/mod.rs").read_text()
        self.assertIn("url: state.embedding_service().url()", health)
        self.assertIn("reachable: reachable_state == ProbeState::Ok", health)

    def test_owned_isolation_restart_requires_different_generation(self):
        health = {"instance": {"boot_generation": "initial"},
                  "embedding": {"reachable_state": "not_configured", "reachable": False,
                                "url": "http://127.0.0.1:7997"},
                  "llm": {"key_configured": False}}
        generations = []
        with patch.object(reliability, "wait_owned_ready", return_value=health):
            observation = reliability.readiness(object(), Path("/offline/config"), [10001, 10002, 10003],
                                                {"source_commit": "a" * 40}, generations)
            self.assertEqual(observation["distinct_owned_generations"], 1)
            self.assertTrue(observation["embedding_disabled"])
            self.assertFalse(observation["embedding_endpoint_owned"])
            self.assertFalse(observation["all_transport_isolation_verified"])
            self.assertNotIn("embedding_disabled_and_endpoint_owned", observation)
            with self.assertRaisesRegex(JourneyError, "BOOT_GENERATION_REUSED"):
                reliability.readiness(object(), Path("/offline/config"), [10001, 10002, 10003],
                                      {"source_commit": "a" * 40}, generations)
            health["instance"]["boot_generation"] = "after-isolation"
            self.assertEqual(reliability.readiness(object(), Path("/offline/config"), [10001, 10002, 10003],
                              {"source_commit": "a" * 40}, generations)["distinct_owned_generations"], 2)

    def test_reviewed_feature_metadata_matches_current_source(self):
        source = Path(__file__).resolve().parents[2] / "crates/mike-base/src/feature_toggles/meta.rs"
        text = source.read_text()
        actual = {}
        for key, scope, applies in re.findall(r'ToggleMeta\s*\{\s*key:\s*"([a-z_]+)",\s*scope:\s*ToggleScope::([A-Za-z]+),\s*applies:\s*ToggleApplies::([A-Za-z]+)', text):
            snake = re.sub(r"(?<!^)(?=[A-Z])", "_", applies).lower()
            actual[key] = (scope.lower(), snake)
        # client_serves(key) is the const constructor for a global, immediate toggle.
        for key in re.findall(r'^\s*client_serves\("([a-z_]+)"\),', text, re.MULTILINE):
            actual[key] = ("global", "immediate")
        self.assertEqual(actual, isolation.FEATURES)


if __name__ == "__main__":
    unittest.main()
