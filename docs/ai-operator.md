# EchoType - AI Operator Guide

This document defines how an AI coding agent should operate EchoType on behalf of a user.

EchoType is designed so a local LLM can handle the full loop:
clone, bootstrap, run, diagnose, fix, test, and submit commits.

## Current State (February 26, 2026)

- EchoType is still in early development.
- The runnable app scaffold is in progress.
- This guide is the contract for how the repo should be structured as implementation lands.

## Operator Workflow

1. Clone the repository.
2. Read `README.md`, this guide, and the docs in `docs/`.
3. Bootstrap dependencies with scripted, non-interactive commands.
4. Run EchoType directly from source.
5. Read logs and diagnostics, then address issues.
6. Run tests/checks before proposing or pushing changes.
7. Commit with clear messages that explain what changed and why.

## Command Contract (Design Requirement)

The project should expose stable command entry points for agents:

- `./scripts/agent/bootstrap` - install project prerequisites and dependencies.
- `./scripts/agent/dev` - run EchoType from source.
- `./scripts/agent/check` - run linting, tests, and verification checks.
- `./scripts/agent/logs` - show recent diagnostics and runtime logs.
- `./scripts/agent/fix` - optional helper workflow for issue-driven repairs.

Requirements for all agent commands:

- Non-interactive by default.
- Deterministic exit codes (`0` success, non-zero failure).
- Clear stderr/stdout output.
- Safe to run repeatedly (idempotent where practical).
- Documented in plain language for humans and agents.

## Logging and Diagnostics Requirements

- Runtime logs must be written to predictable local paths.
- Logs should be machine-readable (for example, line-delimited JSON) with:
  - timestamp
  - level
  - subsystem/component
  - message
  - optional context fields
- Diagnostics should be accessible by command line, not only GUI.

## Prompt Templates For Users

### Install / Setup

```text
Clone https://github.com/3MUl0R/EchoType.git.
Read README.md and docs/ai-operator.md.
Bootstrap the project, run it from source, and report blockers with fixes.
```

### Bug Fix

```text
Open https://github.com/3MUl0R/EchoType.git.
Reproduce the issue, inspect logs, implement a fix, run checks, and commit with a concise technical summary.
```

### Feature Work

```text
Open https://github.com/3MUl0R/EchoType.git.
Implement the requested feature following docs/product-spec.md and docs/tech-stack.md.
Run checks, update docs, and commit the change set.
```

## Repository Design Rules For AI Operability

- Prefer scripted workflows over manual setup steps.
- Keep configuration explicit and version-controlled.
- Avoid hidden state outside the repository where possible.
- Favor machine-readable outputs for checks, logs, and tooling.
- Keep docs task-oriented so a human or agent can execute the same instructions.
