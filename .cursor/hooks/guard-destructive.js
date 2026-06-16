#!/usr/bin/env node
/**
 * Blocks or warns on destructive dev commands (database drop, migrate-reset).
 * stdin: beforeShellExecution hook JSON
 * stdout: { permission, user_message?, agent_message? }
 */
const chunks = [];
process.stdin.setEncoding("utf8");
process.stdin.on("data", (c) => chunks.push(c));
process.stdin.on("end", () => {
  let input = {};
  try {
    input = JSON.parse(chunks.join("") || "{}");
  } catch {
    process.stdout.write(JSON.stringify({ permission: "allow" }));
    process.exit(0);
  }

  const command = (input.command || "").toLowerCase();

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

  const matched = destructive.find((re) => re.test(command));
  if (matched) {
    process.stdout.write(
      JSON.stringify({
        permission: "ask",
        user_message:
          "This command can destroy local database data or event history. Confirm you intend to run it in dev.",
        agent_message:
          "Hook flagged a destructive database/docker command. Prefer `task migrate` for normal schema updates; use migrate-reset only in local dev with user approval.",
      })
    );
    process.exit(0);
  }

  process.stdout.write(JSON.stringify({ permission: "allow" }));
  process.exit(0);
});
