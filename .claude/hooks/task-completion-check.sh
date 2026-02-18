#!/bin/bash
# Hook: Verify task completion before moving on
# Event: UserPromptSubmit

INPUT=$(cat)
PROMPT=$(echo "$INPUT" | jq -r '.prompt // ""')

# Check if prompt mentions moving to the next task
if echo "$PROMPT" | grep -iqE 'next task|move on|proceed|continue to task'; then
  cat << 'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "STOP: Before moving to the next task, verify: 1) Current task is fully implemented, 2) All tests pass (cargo test), 3) Code is formatted (cargo fmt) and linted (cargo clippy), 4) User has explicitly reviewed and approved. Do NOT proceed until all conditions are confirmed."
  }
}
EOF
fi

exit 0
