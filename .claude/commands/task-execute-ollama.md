---
description: Execute a beads task (Ollama-optimized)
argument-hint: <issue-id>
tools:
  - Bash(todd-carl:*)
  - Bash(npm:*)
  - Bash(git:*)
  - Read
  - Edit
  - Write
  - Glob
  - Grep
---

# Task Execution Guide

Execute the given task by:
1. First run `bd show <issue-id>` to see task details
2. Read relevant source files
3. Make necessary code changes
4. Run tests with `npm run lab:test`
5. Run linting with `npm run lab:clean`
6. Close the task with `bd close <issue-id> --reason="Completed"`
