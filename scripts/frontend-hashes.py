#!/usr/bin/env python3
"""Regenerates (or with --check, verifies) apps/tauri/frontend-sha256.json."""
import hashlib, json, pathlib, sys

root = pathlib.Path(__file__).resolve().parent.parent
web = root / "apps/web"
files = [web / n for n in ("index.html", "package.json", "tsconfig.json", "vite.config.ts")]
files += sorted(p for d in ("public", "src") for p in (web / d).rglob("*") if p.is_file())
table = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
target = root / "apps/tauri/frontend-sha256.json"
if "--check" in sys.argv:
    stale = json.loads(target.read_text()) != table
    print("frontend source hashes are stale" if stale else "frontend source hashes verified")
    sys.exit(1 if stale else 0)
target.write_text(json.dumps(dict(sorted(table.items())), indent=2) + "\n")
print(f"{len(table)} files")
