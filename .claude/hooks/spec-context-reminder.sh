#!/bin/bash
# Hook: Remind to read specs before implementing a task
# Event: UserPromptSubmit

INPUT=$(cat)
PROMPT=$(echo "$INPUT" | jq -r '.prompt // ""')

# Check if prompt mentions implementing/working on a task
if echo "$PROMPT" | grep -iqE '(implement|work on|start|begin) task|/implement'; then
  cat << 'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "REMINDER: Before implementing, ensure you have: 1) Read acceptance criteria in requirements.md, 2) Reviewed architecture in design.md, 3) Checked s3sync repo for reusable code, 4) Checked existing tests for patterns."
  }
}
EOF
fi

exit 0
