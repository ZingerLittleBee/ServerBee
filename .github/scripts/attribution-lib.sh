# Shared attribution matchers for CI and Lefthook.
# Sourced by other scripts in this directory. Do not execute directly.
# Bash 3.2 compatible (macOS /bin/bash).

# Known coding-agent names. Keep in sync with check-pr-attribution usage.
ATTRIBUTION_AGENT_RE='Claude|Copilot|Cursor|Codex|OpenAI|Devin|Gemini|Jules|Windsurf|Cline|Aider|OpenCode|Codeium|Anthropic'

# Any email address. Domain must start with a letter so paths like icon@2x.png
# are not treated as addresses.
ATTRIBUTION_EMAIL_RE='[A-Za-z0-9._%+-]+@[A-Za-z][A-Za-z0-9.-]*\.[A-Za-z]{2,}'

# Print Co-authored-by lines from stdin. Always exits 0.
attribution_co_author_lines() {
  grep -iE '^[[:space:]]*Co-authored-by:' || true
}

# Print lines from stdin that contain an email address. Always exits 0.
attribution_email_lines() {
  grep -iE "${ATTRIBUTION_EMAIL_RE}" || true
}

# Print generated-by / made-with / assisted-by / robot-generated lines from
# stdin. Matches "generated/made with|by" plus a known agent name, or the
# signatures "🤖 Generated" and "Assisted-by:". Does not match "generate
# reports". Emails are handled separately by attribution_email_lines.
# Always exits 0.
attribution_generated_lines() {
  grep -iE \
    -e '🤖[[:space:]]*Generated' \
    -e 'Assisted-by:' \
    -e "Generated[[:space:]]+(with|by)[:[:space:]].*(${ATTRIBUTION_AGENT_RE})" \
    -e "Generated-(with|by)[:[:space:]]*(${ATTRIBUTION_AGENT_RE})" \
    -e "Made[[:space:]]+with[:[:space:]].*(${ATTRIBUTION_AGENT_RE})" \
    -e "Made-with[:[:space:]]*(${ATTRIBUTION_AGENT_RE})" \
    || true
}
