#!/usr/bin/env python3
"""Release notes for one version, from CHANGELOG.md.

  release_notes.py --tag v0.1.0 --out notes.md   # the release workflow, on a tag
  release_notes.py --out notes.md                # the workspace version, no tag

Reads the version from `workspace.package.version` in Cargo.toml, refuses a tag that is not
`v<that version>`, and writes the body of CHANGELOG.md's `## [<version>]` section to --out.
Under GitHub Actions it also sets the step output `version`. Stdlib only.
"""

import argparse
import os
import re
import sys
import tomllib
from pathlib import Path
from typing import NoReturn

ROOT = Path(__file__).resolve().parents[2]


def fail(message: str) -> NoReturn:
    print(f"error: release_notes: {message}", file=sys.stderr)
    sys.exit(1)


def workspace_version() -> str:
    manifest = ROOT / "Cargo.toml"
    try:
        return tomllib.loads(manifest.read_text())["workspace"]["package"]["version"]
    except KeyError:
        fail(f"{manifest} has no workspace.package.version")


def section(changelog: str, version: str) -> str:
    """The text under `## [version]`, up to the next `## ` heading or the link list."""
    heading = re.compile(rf"^## \[{re.escape(version)}\][^\n]*\n", re.MULTILINE)
    found = heading.search(changelog)
    if found is None:
        fail(f"CHANGELOG.md has no '## [{version}]' section")
    rest = changelog[found.end() :]
    end = re.search(r"^(## |\[[^\]]+\]: )", rest, re.MULTILINE)
    body = (rest[: end.start()] if end else rest).strip()
    if not body:
        fail(f"CHANGELOG.md's '## [{version}]' section is empty")
    return body + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tag", default="", help="the pushed tag, `v<version>`; empty for none")
    parser.add_argument("--out", type=Path, required=True, help="where to write the notes")
    args = parser.parse_args()

    version = workspace_version()
    if args.tag and args.tag != f"v{version}":
        fail(f"tag {args.tag} does not match workspace version {version} (expected v{version})")

    notes = section((ROOT / "CHANGELOG.md").read_text(), version)
    args.out.write_text(notes)
    print(f"ra-jit {version}: {len(notes.splitlines())} lines of notes -> {args.out}")
    print(notes)

    if output := os.environ.get("GITHUB_OUTPUT"):
        with open(output, "a") as f:
            f.write(f"version={version}\n")


if __name__ == "__main__":
    main()
