---
name: install-codex-hooks
description: Install or update repo-local Codex hooks for ralph-hook-lint in the current repository.
---

Install or refresh `ralph-hook-lint` Codex hooks for the current repository.

When the user asks to enable this plugin for Codex in the current repo:

1. Resolve the plugin root from this skill location.
2. Run `python3 <plugin-root>/scripts/install_codex_hooks.py`.
3. If the user wants verbose hook messages, rerun with `--debug`.
4. Tell the user which files were updated and remind them to restart Codex after the first install.

This installer is idempotent:

- it enables `codex_hooks = true` in `~/.codex/config.toml`
- it creates or updates `<repo>/.codex/hooks.json`
- it preserves unrelated hooks and only replaces `ralph-hook-lint` managed entries
