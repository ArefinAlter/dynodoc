# 011 — Provenance across all six Microsoft and Google editors

6 October 2026. The owner extends decision 010 to Word/Docs, Excel/Sheets and
PowerPoint/Slides. All three pairs belong to the primary provenance product;
this does not restart web-editor Office parity work.

The shared durable queue now identifies an observed value by stable node ID and
field. Spreadsheet changes address individual `cell_N` fields, retaining formula
source rather than calculated results. Presentation changes address shape `text`.
Both use the existing version-1 bundle, immutable receipt and review/merge path.
Paragraph bundles and recovery files retain their existing representation.

Development connectors exist for all six hosts. Excel uses named row ranges,
Sheets uses named row ranges, PowerPoint uses slide/shape tags, and Google Slides
uses native slide/element IDs mapped privately per user and presentation. Google
Slides preserves accessibility descriptions. Copied files require explicit
binding/recovery against matching content; filename/content matching remains a
suggestion mechanism, never silent provenance attachment.

Scope of this increment: existing paragraph text; existing spreadsheet scalar
values and formula source; existing slide shape text. No structural or formatting
capture, date/locale exchange, charts, tables, groups, media, animations, macros or
full background capture. Merged cells require file push. Unsupported structures,
changed cell types, moved/missing identities, changed preview text and conflicting
edits stop sync. Host replacements can affect local formatting. Formula engines
and presentation layouts can differ across Microsoft and Google; native cross-host
acceptance is required before claiming compatibility.

Migration 0024 expands the host constraint without changing existing migration
bytes, event variants or canonical history. Key creation checks the file kind;
the original file/actor/host/access/session/expiry restrictions remain.

Priority order:

1. Deploy Projects/provenance, then accept all three editor pairs in real desktop
   and cloud hosts, including offline retry, recovery, review/merge and pull.
2. Extend structural and formatting operations per format with stable identity
   and explicit conflict recovery, including dates/formula compatibility and
   slide tables/notes/media. Unsupported content must remain recoverable through
   original files and file push.
3. Durable background/local capture, secure pairing, signed distribution and
   Microsoft/Google marketplace acceptance for all six hosts.
4. Project Compare/Activity/search and coordinated multi-file releases.
5. Further notifications/digests, integrations and assessment proposals.

Source/tests are development evidence, not installed-host acceptance or marketplace
publication. See [release evidence](../SIX-EDITOR-PROVENANCE-RELEASE.md).

The subsequent [date-only increment](../SPREADSHEET-DATE-SYNC-RELEASE.md) narrows
the spreadsheet boundary to permit checked civil days through native calendar
functions/timezones. Excel now requires API 1.13 for local merged-area checks.
Timestamps, formatting/type changes, full formula/date/locale compatibility and
installed-host acceptance remain open. The priority order above still applies.
