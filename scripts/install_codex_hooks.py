#!/usr/bin/env python3
"""Install or update repo-local Codex hooks for ralph-hook-lint."""

from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
from pathlib import Path
from typing import Any


PLUGIN_NAME = "ralph-hook-lint"
PLUGIN_ROOT = Path(__file__).resolve().parent.parent
CONFIG_PATH = Path.home() / ".codex" / "config.toml"
MARKER = PLUGIN_NAME


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Install repo-local Codex hooks for ralph-hook-lint."
    )
    parser.add_argument(
        "--repo",
        help="Target repository root. Defaults to the current git root or cwd.",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Append --debug to Codex hook commands.",
    )
    return parser.parse_args()


def resolve_repo_root(explicit_repo: str | None) -> Path:
    if explicit_repo:
        return Path(explicit_repo).expanduser().resolve()

    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode == 0:
        root = result.stdout.strip()
        if root:
            return Path(root).resolve()

    return Path.cwd().resolve()


def ensure_codex_hooks_enabled() -> None:
    CONFIG_PATH.parent.mkdir(parents=True, exist_ok=True)

    if not CONFIG_PATH.exists():
        CONFIG_PATH.write_text("[features]\ncodex_hooks = true\n")
        return

    text = CONFIG_PATH.read_text()
    if re.search(r"(?m)^\s*codex_hooks\s*=\s*true\s*$", text):
        return

    if re.search(r"(?m)^\s*codex_hooks\s*=\s*false\s*$", text):
        text = re.sub(
            r"(?m)^(\s*codex_hooks\s*=\s*)false(\s*)$",
            r"\1true\2",
            text,
            count=1,
        )
        CONFIG_PATH.write_text(text)
        return

    if re.search(r"(?m)^\[features\]\s*$", text):
        text = re.sub(
            r"(?m)^(\[features\]\s*)$",
            "\\1\ncodex_hooks = true",
            text,
            count=1,
        )
        CONFIG_PATH.write_text(text)
        return

    suffix = "" if text.endswith("\n") else "\n"
    CONFIG_PATH.write_text(f"{text}{suffix}\n[features]\ncodex_hooks = true\n")


def shell_command(path: Path, *args: str) -> str:
    return " ".join(shlex.quote(str(part)) for part in (path, *args))


def build_managed_hooks(debug: bool) -> dict[str, list[dict[str, Any]]]:
    setup_command = shell_command(PLUGIN_ROOT / "scripts" / "setup.sh", PLUGIN_NAME)
    snapshot_args = ["--snapshot-turn"]
    lint_args = ["--lint-turn"]
    if debug:
        snapshot_args.append("--debug")
        lint_args.append("--debug")

    return {
        "SessionStart": [
            {
                "matcher": "startup|resume|clear",
                "hooks": [
                    {
                        "type": "command",
                        "command": setup_command,
                        "statusMessage": "Updating ralph-hook-lint",
                    }
                ],
            }
        ],
        "UserPromptSubmit": [
            {
                "hooks": [
                    {
                        "type": "command",
                        "command": shell_command(
                            PLUGIN_ROOT / "bin" / PLUGIN_NAME, *snapshot_args
                        ),
                        "statusMessage": "Tracking changed files for ralph-hook-lint",
                    }
                ],
            }
        ],
        "Stop": [
            {
                "hooks": [
                    {
                        "type": "command",
                        "command": shell_command(PLUGIN_ROOT / "bin" / PLUGIN_NAME, *lint_args),
                        "statusMessage": "Linting changed files with ralph-hook-lint",
                        "timeout": 120,
                    }
                ],
            }
        ],
    }


def is_managed_group(group: dict[str, Any]) -> bool:
    hooks = group.get("hooks", [])
    if not isinstance(hooks, list):
        return False

    for hook in hooks:
        if not isinstance(hook, dict):
            continue
        command = hook.get("command")
        status_message = hook.get("statusMessage")
        if isinstance(command, str) and MARKER in command:
            return True
        if isinstance(status_message, str) and MARKER in status_message:
            return True

    return False


def load_existing_hooks(hooks_path: Path) -> dict[str, Any]:
    if not hooks_path.exists():
        return {"hooks": {}}

    payload = json.loads(hooks_path.read_text())
    if not isinstance(payload, dict):
        raise ValueError(f"{hooks_path} must contain a JSON object.")

    hooks = payload.get("hooks")
    if hooks is None:
        payload["hooks"] = {}
    elif not isinstance(hooks, dict):
        raise ValueError(f"{hooks_path} field 'hooks' must be an object.")

    return payload


def update_repo_hooks(hooks_path: Path, debug: bool) -> None:
    payload = load_existing_hooks(hooks_path)
    hook_groups = payload.setdefault("hooks", {})
    managed_hooks = build_managed_hooks(debug)

    for event_name, new_groups in managed_hooks.items():
        existing_groups = hook_groups.get(event_name, [])
        if not isinstance(existing_groups, list):
            raise ValueError(f"{hooks_path} event '{event_name}' must contain an array.")

        preserved_groups = [
            group
            for group in existing_groups
            if not (isinstance(group, dict) and is_managed_group(group))
        ]
        hook_groups[event_name] = [*preserved_groups, *new_groups]

    hooks_path.parent.mkdir(parents=True, exist_ok=True)
    hooks_path.write_text(json.dumps(payload, indent=2) + "\n")


def main() -> None:
    args = parse_args()
    repo_root = resolve_repo_root(args.repo)
    hooks_path = repo_root / ".codex" / "hooks.json"

    ensure_codex_hooks_enabled()
    update_repo_hooks(hooks_path, args.debug)

    print(f"Updated {hooks_path}")
    print(f"Updated {CONFIG_PATH}")
    print("Restart Codex if this is the first time you have enabled hooks.")


if __name__ == "__main__":
    main()
