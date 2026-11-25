#!/usr/bin/env python3
"""Extract a version section from CHANGELOG.md (Keep a Changelog format)."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


def extract_changelog_section(text: str, version: str) -> str:
    """Return the body of the first `## [version]` section, or empty string if absent."""
    normalized = version.strip()
    if normalized.startswith(("v", "V")):
        normalized = normalized[1:]
    if not normalized:
        return ""

    heading_re = re.compile(rf"^##\s+\[?v?{re.escape(normalized)}\]?(?:\s|$)")
    next_heading_re = re.compile(r"^##\s+")

    lines = text.splitlines()
    start_idx: int | None = None
    for i, line in enumerate(lines):
        if heading_re.match(line):
            start_idx = i + 1
            break

    if start_idx is None:
        return ""

    end_idx = len(lines)
    for i in range(start_idx, len(lines)):
        if next_heading_re.match(lines[i]):
            end_idx = i
            break

    section_lines = lines[start_idx:end_idx]
    while section_lines and not section_lines[0].strip():
        section_lines.pop(0)
    while section_lines and not section_lines[-1].strip():
        section_lines.pop()

    if not section_lines:
        return ""
    return "\n".join(section_lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Extract release notes for a specific version from CHANGELOG.md."
    )
    parser.add_argument("version", help="Version or tag to extract (e.g. 0.2.0-beta.2 or v0.2.0-beta.2)")
    parser.add_argument(
        "--changelog",
        default="CHANGELOG.md",
        help="Path to CHANGELOG.md (default: CHANGELOG.md)",
    )
    parser.add_argument(
        "--output",
        "-o",
        default=None,
        help="Output file path (prints to stdout when omitted)",
    )
    args = parser.parse_args(argv)

    changelog_path = Path(args.changelog)
    if not changelog_path.is_file():
        content = ""
    else:
        content = extract_changelog_section(
            changelog_path.read_text(encoding="utf-8"),
            args.version,
        )

    if args.output is not None:
        out_path = Path(args.output)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(content, encoding="utf-8")
    else:
        sys.stdout.write(content)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
