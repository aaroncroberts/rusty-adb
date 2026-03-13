# Todd-Bishop CLI Skill

Use this skill when executing tasks from Beads issues, invoking Claude Code programmatically, or managing agentic work assignment.

## When to Use

- Executing tasks from a Beads issue tracker
- Sending prompts to Claude Code from a script or automation
- Viewing execution logs and audit trails
- Processing issues for agentic assignment

## Core Commands

### beads-exec - Execute a Beads Task

Execute a task defined in a Beads issue using Claude:

```bash
# Basic execution
todd-bishop beads-exec beads-abc123 --repo owner/repo

# With structured JSON output (captures session_id, cost, duration)
todd-bishop beads-exec beads-abc123 --repo owner/repo --json

# Dry run to preview the prompt
todd-bishop beads-exec beads-abc123 --repo owner/repo --dry-run

# With verification of acceptance criteria
todd-bishop beads-exec beads-abc123 --repo owner/repo --verify

# Check exit code for status (useful for automation)
# 0 = success, 1 = error, 2 = input required (Claude asked a question)
todd-bishop beads-exec beads-abc123 --repo owner/repo --json
echo $?  # Check exit code

# Custom timeout (default 30 minutes)
todd-bishop beads-exec beads-abc123 --repo owner/repo --timeout 60

# Silent mode (no streaming, just final result)
todd-bishop beads-exec beads-abc123 --repo owner/repo --silent

# Add custom context before the task prompt (separated by ---)
todd-bishop beads-exec beads-abc123 --repo owner/repo --prefix-prompt "Focus on performance"

# Add instructions after the task prompt (separated by ---)
todd-bishop beads-exec beads-abc123 --repo owner/repo --postfix-prompt "Run tests when done"

# Combine both for full prompt wrapping
todd-bishop beads-exec beads-abc123 --repo owner/repo \
  --prefix-prompt "You are working in a TypeScript monorepo" \
  --postfix-prompt "Follow existing code style"
```

### invoke - Send a Prompt to Claude

Send a prompt directly to Claude Code:

```bash
# Basic invocation
todd-bishop invoke "Explain this codebase structure"

# With working directory context
todd-bishop invoke "Review this code" --cwd /path/to/project

# JSON output format
todd-bishop invoke "List all functions" --output-format json

# With additional directories for context
todd-bishop invoke "Compare these" --add-dir /other/project

# Silent mode (no streaming)
todd-bishop invoke "Quick question" --silent
```

### task - Process Issue for Assignment

Process a Beads issue for agentic assignment (outputs structured task info):

```bash
# Standard output
todd-bishop task beads-abc123 --repo owner/repo

# JSON output (for automation)
todd-bishop task beads-abc123 --repo owner/repo --json
```

### assign - Quick Task Assignment

Assign a task with description:

```bash
todd-bishop assign "Implement feature X"
todd-bishop assign "Fix bug Y" --assigned-to "developer"
```

### bead-add - Create Beads Issues with AI

Create beads issues from natural language using Claude with the /task-generate skill:

```bash
# Basic usage - creates bead(s) from description
todd-bishop bead-add "Add user authentication with OAuth2" --repo owner/repo

# Dry run to preview the prompt
todd-bishop bead-add "Refactor the API layer" --repo owner/repo --dry-run

# Use a specific model
todd-bishop bead-add "Fix the login bug" --repo owner/repo --model claude:opus

# Silent mode (no streaming output)
todd-bishop bead-add "Add pagination" --repo owner/repo --silent

# Custom timeout (default 10 minutes)
todd-bishop bead-add "Complex feature" --repo owner/repo --timeout 20
```

The command injects the `/task-generate` skill which guides Claude to:
- Analyze the request complexity
- Choose discovery vs implementation mode
- Create properly structured beads with dependencies
- Set appropriate priorities and types

## Viewing Logs

### Execution Logs

```bash
# List all log files
todd-bishop logs list

# View a specific log
todd-bishop logs view todd-bishop-2026-01-12.log

# Logs are stored in ~/.todd/logs/
# Format: bishop-<task-id>-<date>-<time>.log
```

### Audit Logs

```bash
# List audit log files
todd-bishop audit list

# Query audit entries
todd-bishop audit query --beads-id beads-abc123
todd-bishop audit query --session-id abc123

# View statistics
todd-bishop audit stats
```

## Configuration

Config file location: `~/.todd/config.json`

```json
{
  "version": 1,
  "repositories": {
    "owner/repo": {
      "beadsDbPath": "${HOME}/.beads/repo.db"
    }
  },
  "defaults": {
    "appendSystemPrompt": "Be concise.",
    "claude": {
      "path": "/usr/local/bin/claude",
      "defaultTimeout": 600000,
      "maxBudgetUsd": 1.00
    }
  }
}
```

Environment variables can be used: `${VAR_NAME}` syntax is resolved at load time.

## Version Info

```bash
todd-bishop version        # Show todd-bishop version
todd-bishop claude-version # Show Claude Code CLI version
```

## Handling Input Required (Exit Code 2)

When Claude asks a clarifying question during execution, beads-exec exits with code 2.

### JSON Output (--json mode)

```json
{
  "status": "input_required",
  "beadsId": "beads-abc123",
  "sessionId": "session-xyz-789",
  "question": {
    "questions": [
      {
        "question": "Which approach should I use?",
        "options": [
          { "label": "Option A", "description": "First approach" },
          { "label": "Option B", "description": "Second approach" }
        ]
      }
    ]
  },
  "resumeCommand": "beads-resume beads-abc123 --inject '<answer>'"
}
```

### Automation Pattern

```bash
# Execute and check result
result=$(todd-bishop beads-exec beads-abc123 --repo owner/repo --json)
exit_code=$?

case $exit_code in
  0) echo "Success" ;;
  1) echo "Error" ;;
  2)
    # Input required - parse question and handle
    session_id=$(echo "$result" | jq -r '.sessionId')
    question=$(echo "$result" | jq -r '.question.questions[0].question')
    echo "Claude asks: $question"
    # Resume with answer:
    # todd-bishop beads-resume beads-abc123 --inject "Option A"
    ;;
esac
```

## Tips

1. **Use --json for automation**: Captures session_id, status, and question details
2. **Use --dry-run first**: Preview prompts before execution
3. **Check logs after execution**: `todd-bishop logs list` shows recent activity
4. **Verify with acceptance criteria**: `--verify` checks task completion
5. **Handle exit code 2**: Claude asked a question - resume with `--inject`
