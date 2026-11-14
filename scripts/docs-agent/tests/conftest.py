"""Make the docs-agent modules importable — the package is run as a script
(`python scripts/docs-agent`), so there is no installed package to import from."""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

import client


@pytest.fixture(autouse=True)
def no_decisions_call(monkeypatch):
    """No test reaches OpenRouter's decisions endpoint unless it stubs the call."""
    monkeypatch.setattr(client, "decide", lambda state, questions, model=None: None)
