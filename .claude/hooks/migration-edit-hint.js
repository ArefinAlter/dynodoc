#!/usr/bin/env node
/**
 * Claude Code PostToolUse hook (matcher: Write|Edit|MultiEdit).
 * Reminds the agent not to edit applied SQL migrations.
 *
 * stdin:  PostToolUse hook JSON ({ tool_name, tool_input: { file_path } })
 * stdout: { hookSpecificOutput: { hookEventName, additionalContext } }
 */
const chunks = [];
process.stdin.setEncoding("utf8");
process.stdin.on("data", (c) => chunks.push(c));
process.stdin.on("end", () => {
  let input = {};
  try {
    input = JSON.parse(chunks.join("") || "{}");
  } catch {
    process.exit(0);
  }

  const tool = input.tool_name || "";
  const path =
    input.tool_input?.file_path ||
    input.tool_input?.path ||
    "";

  if (!/migrations[/\\].*\.sql$/i.test(path)) {
    process.exit(0);
  }

  if (!/^(Write|Edit|MultiEdit)$/i.test(tool)) {
    process.exit(0);
  }

  process.stdout.write(
    JSON.stringify({
      hookSpecificOutput: {
        hookEventName: "PostToolUse",
        additionalContext:
          "Migration edit detected. Migrations are immutable once applied — if this " +
          "file already ran in any shared environment, add a new numbered migration " +
          "instead of editing it. PoC scope: only 0000–0004 and 0009_views.",
      },
    })
  );
  process.exit(0);
});
