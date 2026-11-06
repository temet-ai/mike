"""Make the docs-agent modules importable — the package is run as a script
(`python scripts/docs-agent`), so there is no installed package to import from."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
