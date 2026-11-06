"""Offline contract checks for the pilot forms; no runtime or journey claims.

Run: python3 -m unittest discover -s scripts/tests -p 'test_pilot_feedback.py'
Requires PyYAML, also used by scripts/knowledge. No network, DB, or build needed.
"""
from __future__ import annotations

import copy
from pathlib import Path
import re
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[2]
FORMS = ROOT / ".github" / "ISSUE_TEMPLATE"
GUIDE = ROOT / "docs" / "PILOT_FEEDBACK.md"
COMMON_FIELDS = {
    "safety": ("checkboxes", "Before submitting"),
    "version": ("input", "Version or build identity"),
    "platform": ("input", "Platform"),
    "storage": ("dropdown", "Storage method"),
    "tried": ("textarea", "What did you try?"),
    "expected": ("textarea", "What did you expect?"),
    "actual": ("textarea", "What actually happened?"),
    "reproduction": ("textarea", "Reproducible non-sensitive example"),
    "diagnostics": ("textarea", "Safe diagnostic summary (optional)"),
}
EXTRA_FIELDS = {
    "install-problem.yml": {"installation": ("dropdown", "Installation method")},
    "runtime-problem.yml": {
        "surface": ("dropdown", "Affected workflow"),
        "daemon_state": ("dropdown", "Observed daemon state"),
        "capture_expectation": ("dropdown", "Was new capture expected?"),
    },
}
FORBIDDEN_EVIDENCE = (
    ".env", "full environment", "prompts", "conversation dumps", "request headers",
    "tokens", "credentials", "full database URLs", "sensitive paths",
)
GUIDE_URL = "https://github.com/temet-ai/mike/blob/main/docs/PILOT_FEEDBACK.md#security-reports"
SENSITIVE_REQUEST = re.compile(
    r"\b(?:attach|copy|include|paste|provide|send|share|upload)\b"
    r"[^.!?\n]{0,80}?(?:\.env\b|raw (?:logs|conversation)|conversation dumps|"
    r"full environment|request headers|(?:api[ -]?)?(?:keys|tokens)|credentials|"
    r"passwords|database urls|sensitive paths)", re.IGNORECASE,
)


class UniqueKeyLoader(yaml.SafeLoader):
    """Reject ambiguous YAML rather than silently using the final duplicate."""


def unique_mapping(loader, node):
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node)
        if key in result:
            raise ValueError(f"duplicate YAML key: {key}")
        result[key] = loader.construct_object(value_node)
    return result


UniqueKeyLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping
)


def load_form(name):
    return yaml.load((FORMS / name).read_text(), Loader=UniqueKeyLoader)


def fields(form):
    return {field["id"]: field for field in form["body"] if "id" in field}


def attribute_text(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for nested in value.values():
            yield from attribute_text(nested)
    elif isinstance(value, list):
        for nested in value:
            yield from attribute_text(nested)


def validate_form(form, name):
    """Keep public collection bounded; a new field needs deliberate review."""
    expected = COMMON_FIELDS | EXTRA_FIELDS[name]
    body = form["body"]
    actual = fields(form)
    errors = []
    if len(body) != len(expected) + 1 or set(actual) != set(expected):
        errors.append("unexpected, duplicate, or missing form fields")
    for field_id, (kind, label) in expected.items():
        field = actual.get(field_id, {})
        attributes = field.get("attributes", {})
        if field.get("type") != kind or attributes.get("label") != label:
            errors.append(f"unreviewed field: {field_id}")
        if field_id not in ("safety", "diagnostics"):
            if field.get("validations", {}).get("required") is not True:
                errors.append(f"required field missing: {field_id}")
        if "value" in attributes:
            errors.append(f"pre-filled evidence: {field_id}")
        for value in attribute_text(attributes):
            if SENSITIVE_REQUEST.search(value):
                errors.append(f"sensitive evidence requested: {field_id}")
    if actual.get("diagnostics", {}).get("validations", {}).get("required"):
        errors.append("diagnostics must remain optional")
    return errors


class PilotFeedbackFormsTest(unittest.TestCase):
    def test_forms_are_bounded_valid_github_issue_forms(self):
        for name in EXTRA_FIELDS:
            with self.subTest(name=name):
                form = load_form(name)
                self.assertEqual(set(form), {"name", "description", "title", "body"})
                for key in ("name", "description", "title"):
                    self.assertIsInstance(form[key], str)
                    self.assertTrue(form[key].strip())
                self.assertEqual(validate_form(form, name), [])
                for field in fields(form).values():
                    self.assertRegex(field["id"], r"^[a-z][a-z0-9_]*$")
                    if field["type"] == "dropdown":
                        choices = field["attributes"]["options"]
                        self.assertGreater(len(choices), 1)
                        self.assertTrue(all(isinstance(choice, str) for choice in choices))
                        self.assertEqual(len(choices), len(set(choices)))
                        self.assertTrue(any("unknown" in choice.lower() for choice in choices))

    def test_warning_precedes_collection_and_routes_security_separately(self):
        for name in EXTRA_FIELDS:
            with self.subTest(name=name):
                form = load_form(name)
                notice = form["body"][0]
                self.assertEqual(notice["type"], "markdown")
                warning = notice["attributes"]["value"]
                self.assertIn("These issues may be public", warning)
                self.assertIn("Never include", warning)
                for forbidden in FORBIDDEN_EVIDENCE:
                    self.assertIn(forbidden, warning)
                self.assertIn("verified private reporting channel, not here", warning)
                self.assertIn(GUIDE_URL, warning)
                checks = fields(form)["safety"]["attributes"]["options"]
                self.assertEqual(len(checks), 2)
                self.assertTrue(all(check["required"] is True for check in checks))
                self.assertIn("private data and secrets", checks[0]["label"])
                self.assertIn("not a security vulnerability report", checks[1]["label"])

    def test_forms_request_synthetic_reproduction_not_raw_output(self):
        for name in EXTRA_FIELDS:
            with self.subTest(name=name):
                inputs = fields(load_form(name))
                self.assertIn("synthetic", inputs["tried"]["attributes"]["description"])
                repro = inputs["reproduction"]["attributes"]
                self.assertIn("disposable test data", repro["description"])
                self.assertIn("cannot share safely", repro["description"])
                self.assertIn("not evidence of an existing bug", repro["placeholder"])
                self.assertIn("no raw command output", inputs["actual"]["attributes"]["description"])
                self.assertIn("no database URL", inputs["storage"]["attributes"]["description"])
                self.assertIn("coarse config source", inputs["diagnostics"]["attributes"]["description"])

    def test_runtime_distinguishes_idle_from_expected_capture(self):
        runtime = fields(load_form("runtime-problem.yml"))
        expectation = runtime["capture_expectation"]["attributes"]
        self.assertIn("No, the client was idle", expectation["options"])
        self.assertIn("Yes, I deliberately sent a synthetic request through mike", expectation["options"])
        self.assertIn("x-claude-code-session-id", expectation["description"])
        self.assertEqual(runtime["daemon_state"]["attributes"]["options"],
                         ["Stopped", "Degraded or failing", "Ready", "Unknown"])

    def test_new_data_collection_field_is_rejected(self):
        form = load_form("install-problem.yml")
        form["body"].append({"type": "textarea", "id": "secrets", "attributes": {
            "label": "Paste your .env file",
        }})
        self.assertIn("unexpected, duplicate, or missing form fields",
                      validate_form(form, "install-problem.yml"))

    def test_sensitive_requests_in_existing_fields_are_rejected(self):
        original = load_form("runtime-problem.yml")
        for request in ("Paste your .env file", "Upload raw conversation dumps",
                        "Share request headers", "Attach raw logs",
                        "Provide full environment output", "Send database URLs",
                        "Include API keys and tokens", "Copy credentials"):
            for attribute in ("label", "description", "placeholder"):
                with self.subTest(request=request, attribute=attribute):
                    form = copy.deepcopy(original)
                    fields(form)["diagnostics"]["attributes"][attribute] = request
                    self.assertIn("sensitive evidence requested: diagnostics",
                                  validate_form(form, "runtime-problem.yml"))

    def test_sensitive_request_in_dropdown_option_is_rejected(self):
        form = load_form("install-problem.yml")
        fields(form)["storage"]["attributes"]["options"].append("Upload your .env file")
        self.assertIn("sensitive evidence requested: storage",
                      validate_form(form, "install-problem.yml"))

    def test_missing_required_input_is_rejected(self):
        original = load_form("install-problem.yml")
        for field_id in ("version", "platform", "storage", "tried", "expected", "actual", "reproduction"):
            with self.subTest(field_id=field_id):
                form = copy.deepcopy(original)
                fields(form)[field_id]["validations"]["required"] = False
                self.assertIn(f"required field missing: {field_id}",
                              validate_form(form, "install-problem.yml"))

    def test_duplicate_yaml_keys_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "duplicate YAML key"):
            yaml.load("name: First\nname: Second\n", Loader=UniqueKeyLoader)


class PilotFeedbackGuideTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.guide = GUIDE.read_text()

    def test_guide_has_required_frontmatter_and_existing_local_links(self):
        metadata = yaml.safe_load(self.guide.split("---", 2)[1])
        self.assertTrue({"title", "status", "created", "updated", "summary"} <= set(metadata))
        for target in re.findall(r"\]\(([^)]+)\)", self.guide):
            if "://" not in target and not target.startswith("#"):
                self.assertTrue((GUIDE.parent / target.split("#")[0]).is_file(), target)

    def test_security_contact_is_honest_and_public_reporting_is_rejected(self):
        self.assertIn("Do not put vulnerability details or exploit material", self.guide)
        self.assertIn("does not provide a verified private reporting", self.guide)
        self.assertIn("withhold the details", self.guide)
        self.assertNotRegex(self.guide, r"mailto:|/security/advisories/new")
        self.assertNotRegex(self.guide, r"[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}")

    def test_evidence_keeps_unrun_cases_and_intervention_visible(self):
        for phrase in ("NOT RUN is not a pass", "AC26: NOT RUN", "AC27: NOT RUN",
                       "FEEDBACK01: NOT RUN", "source SHA", "SHA-256", "exit status",
                       "Time to first capture", "Maintainer intervention", "return use",
                       "predecessor", "independent verifier", "deferred CI"):
            self.assertIn(phrase, self.guide)
        self.assertIn("closes none", self.guide)
        self.assertIn("does not recruit a cohort or authorize publication", self.guide)

    def test_no_unimplemented_diagnostic_command_or_automatic_upload_claim(self):
        self.assertIn("doctor --support-bundle", self.guide)
        self.assertIn("allowlisted JSON summary locally", self.guide)
        self.assertIn("uploads nothing automatically", self.guide)
        self.assertIn("qualification against the exact installed candidate", self.guide)
        self.assertIn("An untested target is unsupported", self.guide)
        self.assertIn("history is lost on restart", self.guide)
        self.assertIn("unauthenticated", self.guide)
        self.assertIn("Missing or malformed session identity", self.guide)


if __name__ == "__main__":
    unittest.main()
