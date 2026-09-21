# Decision 004 — Document version control is the product

Date: 21 September 2026. Status: accepted by the product owner; office-editor
increment implemented, broader Office parity remains ongoing.

The owner explicitly pivoted Dynodoc from research-first software to version
control for rich-text documents and spreadsheets. This supersedes the product
positioning and questionnaire-only scope of decision 000 and the ten-researcher
framing of decision 001. Research integrations remain supported use cases.
It does not silently activate public-consultation, ML or CRDT stages.

Lead product communication with personal drafts, formatting-aware comparison,
selected sharing, overlap review, named versions, attribution and restoration.
Explain blocks through familiar editing actions; implementation details belong
in documentation and advanced history tools.

Keep PostgreSQL and the append-only, stable-ID semantic engine. Page settings,
font descriptors, spreadsheet layout and text-review marks use existing fields;
this release needs no database migration. Never rewrite historical events to
make imported documents resemble a newer editor schema.

Use open-source ProseMirror/Tiptap extensions, native bounded OOXML conversion,
ExcelJS and HyperFormula for common office workflows. Use Paged.js in an isolated
same-origin preview for paginated print/PDF. Editing remains continuous. Paid
Tiptap Pages/Conversion features are not licensed or included by this decision.

Publish a specific compatibility matrix. Do not advertise complete Word/Excel
parity, exact DOCX/XLSX round trips, certified WCAG compliance or arbitrary
400-page imports in a fixed time without measured evidence. Canonical event
history and inline tracked text are distinct features with different guarantees.

Implementation, alternatives, privacy behavior, tests and outstanding work are
recorded in [office editor capabilities](../OFFICE-EDITOR-CAPABILITIES.md).
