#!/usr/bin/env python3
"""Validate project-interop example fixtures against JSON schemas.

Usage:
    python3 validate.py

Exits 0 if all validations pass, 1 otherwise.
"""

import json
import sys
from pathlib import Path

import jsonschema

SCHEMA_DIR = Path(__file__).parent / "schemas"
TESTDATA_DIR = SCHEMA_DIR / "testdata"

def load_json(path: Path) -> dict:
    return json.loads(path.read_text())

def validate(instance_path: Path, schema_path: Path, expect_valid: bool) -> bool:
    schema = load_json(schema_path)
    instance = load_json(instance_path)
    label = instance_path.name

    try:
        jsonschema.validate(instance, schema)
        if expect_valid:
            print(f"  PASS  {label}")
            return True
        else:
            print(f"  FAIL  {label} — expected rejection but document was accepted")
            return False
    except jsonschema.ValidationError as e:
        if not expect_valid:
            print(f"  PASS  {label} (rejected: {e.message[:80]})")
            return True
        else:
            print(f"  FAIL  {label} — {e.message}")
            return False

def main() -> int:
    project_schema = SCHEMA_DIR / "project.schema.json"
    servers_schema = SCHEMA_DIR / "servers.schema.json"
    session_schema = SCHEMA_DIR / "session.schema.json"

    results = []

    # --- Valid project definitions ---
    print("Project definitions (expect valid):")
    for f in sorted(TESTDATA_DIR.glob("valid-*.project.json")):
        results.append(validate(f, project_schema, expect_valid=True))

    # --- Invalid project definitions ---
    print("\nProject definitions (expect invalid):")
    for f in sorted(TESTDATA_DIR.glob("invalid-*.project.json")):
        results.append(validate(f, project_schema, expect_valid=False))

    # --- Valid server registry ---
    print("\nServer registry (expect valid):")
    for f in sorted(TESTDATA_DIR.glob("valid-servers*.json")):
        results.append(validate(f, servers_schema, expect_valid=True))

    # --- Invalid server registry ---
    print("\nServer registry (expect invalid):")
    for f in sorted(TESTDATA_DIR.glob("invalid-http-*.json")):
        results.append(validate(f, servers_schema, expect_valid=False))

    # --- Valid sessions ---
    print("\nAgent sessions (expect valid):")
    for f in sorted(TESTDATA_DIR.glob("valid-session*.json")):
        results.append(validate(f, session_schema, expect_valid=True))

    # --- Invalid sessions ---
    print("\nAgent sessions (expect invalid):")
    for f in sorted(TESTDATA_DIR.glob("invalid-bad-session-*.json")):
        results.append(validate(f, session_schema, expect_valid=False))

    # --- Summary ---
    passed = sum(results)
    total = len(results)
    print(f"\n{'='*40}")
    print(f"  {passed}/{total} checks passed")
    print(f"{'='*40}")

    return 0 if all(results) else 1

if __name__ == "__main__":
    sys.exit(main())
