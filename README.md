# ralph-hook-lint

Zero dependencies lighting fast universal lint hook for your ~~Ralph Wiggum~~ agent loop.

See also format hook: [ralph-hook-fmt](https://github.com/chenhunghan/ralph-hook-fmt)

<p align="center">
  <img src="https://github.com/user-attachments/assets/7c63516e-ed02-4d98-952d-a642215cb722" alt="Ralph Wiggum" />
</p>

## What it does

Runs the same Rust lint hook in both Claude Code and Codex.

- **Claude Code**: collects files touched by `Write`/`Edit` and lints them on `Stop`
- **Codex**: snapshots the workspace on `UserPromptSubmit`, diffs it on `Stop`, and lints files changed during that turn

If lint errors are found, the agent is prompted to fix them before wrapping up.

## Supported Languages

- **JavaScript/TypeScript**: `oxlint` > `biome` > `eslint` > `npm run lint` (in order of preference)
- **Rust**: `clippy`
- **Python**: `ruff` > `mypy` > `pylint` > `flake8` (in order of preference)
- **Java**: Maven (`pmd:check` > `spotbugs:check`) or Gradle (`pmdMain` > `spotbugsMain`)
- **Go**: `golangci-lint` > `staticcheck` > `go vet` (in order of preference)

## Installation

### Claude Code

```bash
claude plugin marketplace add https://github.com/chenhunghan/ralph-hook-lint.git
claude plugin install ralph-hook-lint
```

### Codex

Codex local plugin publishing is still marketplace-file based, so the install flow is:

1. Clone this repo somewhere stable.
2. Add a local Codex marketplace entry that points at that clone.
3. Install the plugin from the Codex plugin directory.
4. In the target repo, ask Codex to use `install-codex-hooks`, or run:

```bash
python3 /path/to/ralph-hook-lint/scripts/install_codex_hooks.py
```

The installer:

- enables `codex_hooks = true` in `~/.codex/config.toml`
- creates or updates `<repo>/.codex/hooks.json`
- preserves unrelated hooks

Example personal marketplace entry:

```json
{
  "name": "local-plugins",
  "interface": {
    "displayName": "Local Plugins"
  },
  "plugins": [
    {
      "name": "ralph-hook-lint",
      "source": {
        "source": "local",
        "path": "./code/ralph-hook-lint"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Productivity"
    }
  ]
}
```

## Update Plugin

### Claude Code

```bash
claude plugin marketplace update ralph-hook-lint
claude plugin update ralph-hook-lint@ralph-hook-lint
```

### Codex

Once Codex hooks are installed, `SessionStart` runs [`scripts/setup.sh`](./scripts/setup.sh), which keeps the downloaded binary under the installed plugin directory up to date automatically.

If you move the plugin clone or reinstall it from a different marketplace path, rerun the Codex installer so repo-local hooks point at the right plugin location again.

## How It Works

### Claude Code

By default, Claude uses a **two-phase deferred linting** approach:

1. **Collect phase** (`PostToolUse`): After every `Write`/`Edit`, file paths are collected without running linters.
2. **Lint phase** (`Stop`): When the agent finishes, all collected files are linted at once in strict mode.

This lets the agent work freely during editing and catches all lint errors before the turn ends.

### Codex

Current Codex pre/post tool hooks only expose `Bash`, so this plugin uses a turn-scoped flow instead:

1. **Update phase** (`SessionStart`): refresh the downloaded `ralph-hook-lint` binary if a newer release exists
2. **Snapshot phase** (`UserPromptSubmit`): record the current fingerprint of supported source files under the repo
3. **Lint phase** (`Stop`): diff the snapshot against the current workspace and lint only files changed in that turn

If Codex already continued once from a failing `Stop` hook, `ralph-hook-lint` will not trigger a second automatic continuation, which avoids endless stop-hook loops.

## Lenient Mode

Disabled by default. The `--lenient` flag suppresses unused variable/import rules, which is useful when running lint on every `Edit` event instead of deferring to `Stop`. Intermediate edit states often have unused variables/imports that will be resolved in later edits.

To run lint on every edit with lenient mode, change `hooks.json` to:

1. Open `~/.claude/plugins/ralph-hook-lint/hooks/hooks.json`
2. Replace the `PostToolUse` collect hook with a direct lint:
   ```json
   "PostToolUse": [
     {
       "matcher": "Write|Edit",
       "hooks": [
         {
           "type": "command",
           "command": "${CLAUDE_PLUGIN_ROOT}/bin/ralph-hook-lint --lenient"
         }
       ]
     }
   ]
   ```

This gives more immediate feedback but may block parallel editing.

For Codex, you can get the same behavior by editing the generated `<repo>/.codex/hooks.json` and appending `--lenient` to the `--lint-turn` command.

## Debug Mode

By default, the hook only outputs `systemMessage` when blocking (lint errors found). To see all diagnostic messages, add `--debug` to the command in `hooks.json`:

```json
"command": "${CLAUDE_PLUGIN_ROOT}/bin/ralph-hook-lint --lint-collected --debug"
```

For Codex, rerun the installer with `--debug`:

```bash
python3 /path/to/ralph-hook-lint/scripts/install_codex_hooks.py --debug
```
