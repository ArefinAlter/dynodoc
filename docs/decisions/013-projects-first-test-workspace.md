# 013 — Projects as the product front page

Date: 10 October 2026. Status: accepted owner direction.

After authentication, Dynodoc opens Projects. Each private Project collects multiple
rich-text files, their proposed changes and history. The repository interface is the
primary workspace; the existing document, spreadsheet and presentation editors are
available under a separate Editors navigation item and Web editor file tab.

Profiles initially contain a display name and short bio. The owner explicitly chose
visibility only to people sharing a Project. Profile responses never contain email;
knowing a profile ID does not grant access. Existing authorized membership-management
views still show the addresses needed to manage invitations and access.

Testers may upload originals of at most **3,000,000 bytes (3 MB)**. Enforce this before
client parsing and in server upload/conversion/import paths. Derived provenance JSON,
expanded archive budgets and accumulated document state have separate limits. This
restriction is not evidence of bounded memory for future 100 MB documents, nor a
quota on the total content produced by successive edits. Existing retained files
and history are not deleted or made unreadable by lowering the upload limit.

The first interface increment exposes the implemented per-file main state, private
drafts, pull requests, checked merge/diff and revision recovery. It must not present
these as portable project-wide Git branches or atomic multi-file commits. Those
require the part 3 contracts in ENGINE-EVOLUTION-PLAN.md. Private drafts remain
private until submitted. All write and history permissions remain server enforced.

Screenshots supplied by the owner guide information hierarchy: global navigation,
profile sidebar, repository file table and repository tabs. Dynodoc retains its own
visual language and shows only actions backed by the current implementation.
