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

Requires Claude Code with `asyncRewake` hook support (released January 2026 or later).

```bash
claude plugin marketplace add https://github.com/chenhunghan/ralph-hook-lint.git
claude plugin install ralph-hook-lint
```

### Codex

Codex local plugin publishing is still marketplace-file based. As of today, the docs describe marketplace-based installation and plugin caching, but do not document a `codex plugin install` CLI command.

The easiest path is the bootstrap script, which clones or updates the plugin under `~/.codex/plugins/ralph-hook-lint`, adds a local marketplace entry, enables `codex_hooks`, and writes repo-local hooks:

```bash
curl -fsSL https://raw.githubusercontent.com/chenhunghan/ralph-hook-lint/main/scripts/install_codex.sh | bash
```

Target a different repo or turn on verbose hook diagnostics:

```bash
curl -fsSL https://raw.githubusercontent.com/chenhunghan/ralph-hook-lint/main/scripts/install_codex.sh | bash -s -- --repo /absolute/path/to/repo --debug
```

If you already cloned this repo locally, run the script directly instead:

```bash
bash scripts/install_codex.sh --repo /absolute/path/to/repo
```

After the script finishes:

1. Restart Codex if this is your first plugin or hook install.
2. In the Codex app, open the Plugin Directory and enable/install `ralph-hook-lint` from your local marketplace if you want it visible there.
3. Open the target repo in Codex. The repo-local hooks are already configured.

### Codex Manual Fallback

If you prefer to wire everything up by hand, the equivalent flow is:

1. Clone this repo somewhere stable.
2. Add a local Codex marketplace entry that points at that clone.
3. Enable or install the plugin from that marketplace in the Codex app.
4. Enable hooks:

```bash
codex features enable codex_hooks
```

This is equivalent to setting the following in `~/.codex/config.toml`:

```toml
[features]
codex_hooks = true
```

5. In the target repo, create `<repo>/.codex/hooks.json`:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|resume|clear",
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/ralph-hook-lint/scripts/setup.sh ralph-hook-lint",
            "statusMessage": "Updating ralph-hook-lint"
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/ralph-hook-lint/bin/ralph-hook-lint --snapshot-turn"
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/ralph-hook-lint/bin/ralph-hook-lint --lint-turn",
            "timeout": 120
          }
        ]
      }
    ]
  }
}
```

Replace `/path/to/ralph-hook-lint` with the absolute path to your local clone.

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

Once Codex hooks are configured, `SessionStart` runs [`scripts/setup.sh`](./scripts/setup.sh), which keeps the downloaded binary under the installed plugin directory up to date automatically.

If you move the plugin clone or reinstall it from a different marketplace path, update `<repo>/.codex/hooks.json` so the commands point at the right plugin location again.

## How It Works

### Claude Code

By default, Claude uses a **two-phase background linting** approach:

1. **Collect phase** (`PostToolUse`): After every `Write`/`Edit`, file paths are collected without running linters.
2. **Lint phase** (`Stop`, `asyncRewake`): When the agent finishes, lint runs in the background while the stop transition completes cleanly. On lint failure, Claude is woken with the diagnostics as a system reminder; on success, nothing is surfaced.

Because the `Stop` hook no longer blocks, the agent's in-flight reasoning isn't displaced by lint output — the wake-up arrives as additive context on top of the existing conversation, not as a replacement for the stop intent.

The Stop hook has a 30-second timeout. Lint runs that exceed it are killed and silently dropped — slow linters won't surface stale wake-ups mid next turn.

### Codex

Current Codex pre/post tool hooks only expose `Bash`, so this plugin uses a turn-scoped flow instead:

1. **Update phase** (`SessionStart`): refresh the downloaded `ralph-hook-lint` binary if a newer release exists
2. **Snapshot phase** (`UserPromptSubmit`): record the current fingerprint of supported source files under the repo
3. **Lint phase** (`Stop`): diff the snapshot against the current workspace and lint only files changed in that turn

If Codex already continued once from a failing `Stop` hook, `ralph-hook-lint` will not trigger a second automatic continuation, which avoids endless stop-hook loops.

## Lenient Mode

Disabled by default. The `--lenient` flag suppresses unused variable/import rules, which is useful when running lint on every `Edit` event instead of deferring to `Stop`. Intermediate edit states often have unused variables/imports that will be resolved in later edits.

Note: with the default `asyncRewake` flow, strict-mode failures arrive as a non-blocking wake-up rather than a Stop block, so lenient is mostly relevant for the eager-on-edit `PostToolUse` setup below where lint runs synchronously after every edit.

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

For Codex, you can get the same behavior by editing `<repo>/.codex/hooks.json` and appending `--lenient` to the `--lint-turn` command.

## Debug Mode

Add `--debug` to surface diagnostic messages for passing runs.

For Claude Code (`asyncRewake` flow), pass diagnostics are written to the hook's stderr — visible when running the binary by hand, but not consumed by the agent (the agent only sees output when lint fails and the wake-up fires). Lint-failure diagnostics always reach the agent regardless of `--debug`.

```json
"command": "${CLAUDE_PLUGIN_ROOT}/bin/ralph-hook-lint --lint-collected --debug"
```

For Codex, edit `<repo>/.codex/hooks.json` and append `--debug` to both the `--snapshot-turn` and `--lint-turn` commands. Codex uses the synchronous `decision:block` protocol, so `--debug` surfaces pass messages as `systemMessage` on stdout.
