#!/usr/bin/env bash
set -euo pipefail

PLUGIN_NAME="ralph-hook-lint"
PLUGIN_REPO="https://github.com/chenhunghan/ralph-hook-lint.git"
PLUGIN_DIR="${HOME}/.codex/plugins/${PLUGIN_NAME}"
MARKETPLACE_PATH="${HOME}/.agents/plugins/marketplace.json"
TARGET_REPO="${PWD}"
DEBUG=0

usage() {
  cat <<'EOF'
Install ralph-hook-lint for Codex.

Usage:
  install_codex.sh [--repo /absolute/path/to/repo] [--debug]
  install_codex.sh /absolute/path/to/repo [--debug]

Options:
  --repo <path>  Target repository for .codex/hooks.json (defaults to current directory)
  --debug        Append --debug to Codex hook commands
  -h, --help     Show this help
EOF
}

resolve_path() {
  python3 - "$1" <<'PY'
import os
import sys

print(os.path.abspath(os.path.expanduser(sys.argv[1])))
PY
}

ensure_plugin_checkout() {
  mkdir -p "$(dirname "$PLUGIN_DIR")"

  if [ -e "$PLUGIN_DIR" ] && [ ! -d "$PLUGIN_DIR/.git" ]; then
    echo "[ralph-hook-lint] $PLUGIN_DIR exists but is not a git checkout." >&2
    exit 1
  fi

  if [ -d "$PLUGIN_DIR/.git" ]; then
    git -C "$PLUGIN_DIR" fetch --depth 1 origin
    default_ref="$(git -C "$PLUGIN_DIR" symbolic-ref refs/remotes/origin/HEAD 2>/dev/null || true)"
    default_branch="${default_ref##refs/remotes/origin/}"
    if [ -z "$default_branch" ] || [ "$default_branch" = "$default_ref" ]; then
      default_branch="main"
    fi
    git -C "$PLUGIN_DIR" checkout "$default_branch"
    git -C "$PLUGIN_DIR" pull --ff-only origin "$default_branch"
  else
    git clone --depth 1 "$PLUGIN_REPO" "$PLUGIN_DIR"
  fi
}

enable_codex_hooks_feature() {
  if command -v codex >/dev/null 2>&1; then
    codex features enable codex_hooks >/dev/null
    return
  fi

  python3 - "${HOME}/.codex/config.toml" <<'PY'
from pathlib import Path
import re
import sys

config_path = Path(sys.argv[1]).expanduser()
config_path.parent.mkdir(parents=True, exist_ok=True)

if not config_path.exists():
    config_path.write_text("[features]\ncodex_hooks = true\n")
    raise SystemExit(0)

text = config_path.read_text()
if re.search(r"(?m)^\s*codex_hooks\s*=\s*true\s*$", text):
    raise SystemExit(0)

if re.search(r"(?m)^\s*codex_hooks\s*=\s*false\s*$", text):
    text = re.sub(
        r"(?m)^(\s*codex_hooks\s*=\s*)false(\s*)$",
        r"\1true\2",
        text,
        count=1,
    )
    config_path.write_text(text)
    raise SystemExit(0)

if re.search(r"(?m)^\[features\]\s*$", text):
    text = re.sub(r"(?m)^(\[features\]\s*)$", "\\1\ncodex_hooks = true", text, count=1)
    config_path.write_text(text)
    raise SystemExit(0)

suffix = "" if text.endswith("\n") else "\n"
config_path.write_text(f"{text}{suffix}\n[features]\ncodex_hooks = true\n")
PY
}

update_marketplace() {
  python3 - "$MARKETPLACE_PATH" "$PLUGIN_NAME" <<'PY'
from pathlib import Path
import json
import sys

marketplace_path = Path(sys.argv[1]).expanduser()
plugin_name = sys.argv[2]
plugin_entry = {
    "name": plugin_name,
    "source": {
        "source": "local",
        "path": f"./.codex/plugins/{plugin_name}",
    },
    "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL",
    },
    "category": "Productivity",
}

if marketplace_path.exists():
    payload = json.loads(marketplace_path.read_text())
else:
    payload = {
        "name": "local-plugins",
        "interface": {"displayName": "Local Plugins"},
        "plugins": [],
    }

payload.setdefault("name", "local-plugins")
payload.setdefault("interface", {})
payload["interface"].setdefault("displayName", "Local Plugins")
plugins = payload.setdefault("plugins", [])
plugins = [entry for entry in plugins if entry.get("name") != plugin_name]
plugins.append(plugin_entry)
payload["plugins"] = plugins

marketplace_path.parent.mkdir(parents=True, exist_ok=True)
marketplace_path.write_text(json.dumps(payload, indent=2) + "\n")
PY
}

update_repo_hooks() {
  local hooks_path="${TARGET_REPO}/.codex/hooks.json"
  local debug="${DEBUG}"

  python3 - "$hooks_path" "$PLUGIN_DIR" "$PLUGIN_NAME" "$debug" <<'PY'
from pathlib import Path
import json
import shlex
import sys

hooks_path = Path(sys.argv[1]).expanduser()
plugin_dir = Path(sys.argv[2]).expanduser()
plugin_name = sys.argv[3]
debug = sys.argv[4] == "1"


def command(*parts: str) -> str:
    return " ".join(shlex.quote(part) for part in parts)


snapshot_args = ["--snapshot-turn"]
lint_args = ["--lint-turn"]
if debug:
    snapshot_args.append("--debug")
    lint_args.append("--debug")

managed_hooks = {
    "SessionStart": [
        {
            "matcher": "startup|resume|clear",
            "hooks": [
                {
                    "type": "command",
                    "command": command(str(plugin_dir / "scripts" / "setup.sh"), plugin_name),
                    "statusMessage": f"Updating {plugin_name}",
                }
            ],
        }
    ],
    "UserPromptSubmit": [
        {
            "hooks": [
                {
                    "type": "command",
                    "command": command(str(plugin_dir / "bin" / plugin_name), *snapshot_args),
                }
            ],
        }
    ],
    "Stop": [
        {
            "hooks": [
                {
                    "type": "command",
                    "command": command(str(plugin_dir / "bin" / plugin_name), *lint_args),
                    "timeout": 120,
                }
            ],
        }
    ],
}

if hooks_path.exists():
    payload = json.loads(hooks_path.read_text())
else:
    payload = {"hooks": {}}

hooks = payload.setdefault("hooks", {})


def is_managed_group(group: dict) -> bool:
    for hook in group.get("hooks", []):
        command_value = hook.get("command")
        status_message = hook.get("statusMessage")
        if isinstance(command_value, str) and plugin_name in command_value:
            return True
        if isinstance(status_message, str) and plugin_name in status_message:
            return True
    return False


for event_name, new_groups in managed_hooks.items():
    existing_groups = hooks.get(event_name, [])
    preserved_groups = [
        group
        for group in existing_groups
        if not (isinstance(group, dict) and is_managed_group(group))
    ]
    hooks[event_name] = [*preserved_groups, *new_groups]

hooks_path.parent.mkdir(parents=True, exist_ok=True)
hooks_path.write_text(json.dumps(payload, indent=2) + "\n")
PY
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --repo)
      if [ "$#" -lt 2 ]; then
        echo "--repo requires a path" >&2
        exit 1
      fi
      TARGET_REPO="$2"
      shift 2
      ;;
    --debug)
      DEBUG=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      TARGET_REPO="$1"
      shift
      ;;
  esac
done

if ! command -v git >/dev/null 2>&1; then
  echo "[ralph-hook-lint] git is required." >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "[ralph-hook-lint] python3 is required." >&2
  exit 1
fi

TARGET_REPO="$(resolve_path "$TARGET_REPO")"
if [ ! -d "$TARGET_REPO" ]; then
  echo "[ralph-hook-lint] target repo does not exist: $TARGET_REPO" >&2
  exit 1
fi

ensure_plugin_checkout
enable_codex_hooks_feature
update_marketplace
update_repo_hooks

cat <<EOF
[ralph-hook-lint] Codex bootstrap complete.

Plugin source:
  $PLUGIN_DIR

Marketplace:
  $MARKETPLACE_PATH

Repo hooks:
  $TARGET_REPO/.codex/hooks.json

Next:
1. Restart Codex if this is your first plugin or hook install.
2. In the Codex app, open the Plugin Directory and enable/install "$PLUGIN_NAME" from your local marketplace if you want it visible there.
3. Open $TARGET_REPO in Codex. The repo-local hooks are already configured.
EOF
