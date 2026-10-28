# Dynodoc 0.1.0-alpha.1 acceptance

This release packages the desktop workflow manager, including the responsive panel fixes merged
in PR #4 and corrected node identity handling for multiple linked document copies (decision 023).
Editing belongs in the Office add-ins and Google extensions; the full workspace opens in the
system browser. Installed-agent adapters use official provider CLIs and their own sign-in. Grok Build uses a separate dedicated profile; see the [setup guide](acceptance/grok-subscription.md).

Engine, PostgreSQL and Office codec components retain their previously verified versions. The
engine source tree, migrations and root Cargo manifests exactly match the pinned 0.0.1b component
build; there is no engine migration in this desktop release. The app licence remains proprietary;
engine and bundled third-party notices retain their respective licences.

Validation receipts and Windows asset hashes are recorded with the release after native checks.
