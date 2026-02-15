# ralph-hook-lint

Zero dependencies lighting fast universal lint hook for your ~~Ralph Wiggum~~ agent loop.

See also format hook: [ralph-hook-fmt](https://github.com/chenhunghan/ralph-hook-fmt)

<p align="center">
  <img src="https://github.com/user-attachments/assets/7c63516e-ed02-4d98-952d-a642215cb722" alt="Ralph Wiggum" />
</p>

## What it does

Lints after every `Write`/`Edit` operation in Claude Code. If lint errors are found, the agent is prompted to fix them.

## Supported Languages

- **JavaScript/TypeScript**: `oxlint` > `biome` > `eslint` > `npm run lint` (in order of preference)
- **Rust**: `clippy`
- **Python**: `ruff` > `mypy` > `pylint` > `flake8` (in order of preference)
- **Java**: Maven (`pmd:check` > `spotbugs:check`) or Gradle (`pmdMain` > `spotbugsMain`)
- **Go**: `golangci-lint` > `staticcheck` > `go vet` (in order of preference)

## Installation

```bash
claude plugin marketplace add chenhunghan/ralph-hook-lint
claude plugin install ralph-hook-lint
```

## Update Plugin

```bash
claude plugin marketplace update ralph-hook-lint
claude plugin update ralph-hook-lint@ralph-hook-lint
```

## Lenient Mode

Enabled by default. When the agent edits files in multiple steps, intermediate states may have unused variables/imports. The `--lenient` flag suppresses these rules so the agent can work incrementally without being blocked mid-edit.

To disable lenient mode, remove `--lenient` from the command in `hooks.json`:

1. Open `~/.claude/plugins/ralph-hook-lint/hooks/hooks.json`
2. Change the command from:
   ```json
   "command": "${CLAUDE_PLUGIN_ROOT}/bin/ralph-hook-lint --lenient"
   ```
   to:
   ```json
   "command": "${CLAUDE_PLUGIN_ROOT}/bin/ralph-hook-lint"
   ```

## Debug Mode

By default, the hook only outputs `systemMessage` when blocking (lint errors found). To see all diagnostic messages, add `--debug` to the command in `hooks.json`:

```json
"command": "${CLAUDE_PLUGIN_ROOT}/bin/ralph-hook-lint --lenient --debug"
```
