#!/usr/bin/env node
/**
 * Reminds the agent not to edit applied SQL migrations after file writes.
 * stdin: postToolUse hook JSON
 * stdout: { additional_context? }
 */
const chunks = [];
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => chunks.push(chunk));
process.stdin.on("end", () => {
  let input = {};
  try {
    input = JSON.parse(chunks.join("") || "{}");
  } catch {
    process.exit(0);
  }

  const tool = String(input.tool_name || "");
  const path =
    input.tool_input?.path ||
    input.tool_input?.file_path ||
    input.tool_input?.target_notebook ||
    "";

  if (!/migrations[/\\].*\.sql$/i.test(path)) {
    process.exit(0);
  }

  if (!/^(Write|StrReplace|EditNotebook)$/i.test(tool)) {
    process.exit(0);
  }

  process.stdout.write(
    JSON.stringify({
      additional_context:
        "Migration edit detected. Migrations are immutable once applied - if this file already ran in any shared environment, add a new numbered migration instead of editing it. PoC scope only includes 0000-0004 and 0009_views.",
    })
  );
  process.exit(0);
});
