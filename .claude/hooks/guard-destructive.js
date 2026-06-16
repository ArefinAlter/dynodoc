#!/usr/bin/env node
/**
 * Claude Code PreToolUse hook (matcher: Bash).
 * Asks for confirmation on destructive dev commands (DB drop, migrate-reset,
 * docker volume removal, event-log mutation).
 *
 * stdin:  PreToolUse hook JSON ({ tool_name, tool_input: { command } })
 * stdout: { hookSpecificOutput: { hookEventName, permissionDecision, permissionDecisionReason } }
 */
const chunks = [];
process.stdin.setEncoding("utf8");
process.stdin.on("data", (c) => chunks.push(c));
process.stdin.on("end", () => {
  let input = {};
  try {
    input = JSON.parse(chunks.join("") || "{}");
  } catch {
    allow();
    return;
  }

  const command = (input.tool_input?.command || "").toLowerCase();

  const destructive = [
    /sqlx\s+database\s+drop/,
    /migrate-reset/,
    /task\s+migrate-reset/,
    /docker\s+compose\s+down\s+.*-v/,
    /docker\s+volume\s+rm/,
    /drop\s+database/,
    /truncate\s+table\s+event/,
    /delete\s+from\s+event/,
  ];

  if (destructive.some((re) => re.test(command))) {
    process.stdout.write(
      JSON.stringify({
        hookSpecificOutput: {
          hookEventName: "PreToolUse",
          permissionDecision: "ask",
          permissionDecisionReason:
            "This command can destroy local database data or event history. " +
            "Prefer `task migrate` for normal schema updates; use migrate-reset " +
            "only in local dev with explicit approval.",
        },
      })
    );
    process.exit(0);
  }

  allow();
});

function allow() {
  process.stdout.write(
    JSON.stringify({
      hookSpecificOutput: {
        hookEventName: "PreToolUse",
        permissionDecision: "allow",
      },
    })
  );
  process.exit(0);
}
