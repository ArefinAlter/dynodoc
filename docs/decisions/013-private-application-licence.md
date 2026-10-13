# Decision 013: the application is private and proprietary

Date: 13 October 2026. Owner: Verne.

## Decision

- The Dynodoc web workspace, website and editor connectors stay in a private repository
  and are not published as open source. The previous AGPL-3.0-or-later licence text and
  metadata are removed from the repository, the frontend package and the generated API
  description; the repository carries a proprietary notice with copyright held by Verne Ltd.
- The engine, HTTP API and command-line tools remain open source under the MIT licence in
  the public repository `ArefinAlter/dynodoc-engine`, with copyright held by Verne Ltd.
- The public site no longer offers a source archive; `/source` redirects to the engine
  repository.

## Consequences and open points

- Revisions of the application that were already distributed under the AGPL stay available
  to their recipients under those terms; this decision governs the repository from the
  commit that introduces it.
- Third-party dependencies keep their own licences. `hyperformula` is offered under
  GPL-3.0-only or a commercial licence and ships in the workspace bundle delivered to
  browsers; using it in a closed-source product requires the commercial licence or a
  replacement. This is recorded, not resolved, by this decision.
- Legal pages must not describe the workspace as open source or as AGPL-licensed
  (see LEGAL-PAGES.md).
