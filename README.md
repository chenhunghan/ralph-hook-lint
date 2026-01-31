# ralph-hook-lint

Zero dependencies lighting fast universal lint hook for your ~~Ralph Wiggum~~ agent loop.

![EOX_2n_WsAAhdZE](https://github.com/user-attachments/assets/7c63516e-ed02-4d98-952d-a642215cb722)

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
