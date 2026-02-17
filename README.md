# ralph-hook-lint

Zero dependencies lighting fast universal lint hook for your ~~Ralph Wiggum~~ agent loop.

See also format hook: [ralph-hook-fmt](https://github.com/chenhunghan/ralph-hook-fmt)

<p align="center">
  <img src="https://github.com/user-attachments/assets/7c63516e-ed02-4d98-952d-a642215cb722" alt="Ralph Wiggum" />
</p>

## What it does

Collects files touched by `Write`/`Edit` operations and lints them all when the agent turn ends (`Stop` event). If lint errors are found, the agent is prompted to fix them.

## Supported Languages

- **JavaScript/TypeScript**: `oxlint` > `biome` > `eslint` > `npm run lint` (in order of preference)
- **Rust**: `clippy`
- **Python**: `ruff` > `mypy` > `pylint` > `flake8` (in order of preference)
- **Java**: Maven (`pmd:check` > `spotbugs:check`) or Gradle (`pmdMain` > `spotbugsMain`)
- **Go**: `golangci-lint` > `staticcheck` > `go vet` (in order of preference)

## Installation

```bash
claude plugin marketplace add https://github.com/chenhunghan/ralph-hook-lint.git
claude plugin install ralph-hook-lint
```

## Update Plugin

```bash
claude plugin marketplace update ralph-hook-lint
claude plugin update ralph-hook-lint@ralph-hook-lint
```

## How It Works

By default, the hook uses a **two-phase deferred linting** approach:

1. **Collect phase** (`PostToolUse`): After every `Write`/`Edit`, file paths are collected without running linters.
2. **Lint phase** (`Stop`): When the agent finishes, all collected files are linted at once in strict mode.

This lets the agent work freely during editing and catches all lint errors before the turn ends.

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

## Debug Mode

By default, the hook only outputs `systemMessage` when blocking (lint errors found). To see all diagnostic messages, add `--debug` to the command in `hooks.json`:

```json
"command": "${CLAUDE_PLUGIN_ROOT}/bin/ralph-hook-lint --lint-collected --debug"
```
