package online.dynodoc.client

// Pure functions (a plain JVM unit test covers them): the token this app adds to its web view's own
// User-Agent, `DynodocApp/<major.minor.patch> (android)`. The server's app sign-in finish page and
// its sign-in routes work only for a window that carries it (docs/decisions/017).
//
// ONE grammar for the version, shared with the Rust shell (`signin::normalise_version`) and the web
// app that reads it (`appKind` in frontend/researcher/src/lib/app-signin.ts), with fixtures all
// three run (apps/desktop/fixtures/app-token.json): `<major>.<minor>.<patch>`, each one to nine
// ASCII digits. The leading dotted triple of the app's version name is kept and anything after it
// is dropped on purpose (`0.1.0-beta.1` gives `0.1.0`, `10.0.0+build5` gives `10.0.0`, `2026.10.22`
// stays); with no such triple the answer is `0.0.0`. The token is never one the server refuses.
object AppToken {
  // `\d` is ASCII only on the JVM unless a flag says otherwise; the grammar means ASCII digits.
  private val LEADING = Regex("""^(\d{1,9})\.(\d{1,9})\.(\d{1,9})(?!\d)""")
  private const val NAME = "DynodocApp/"
  private const val NONE = "0.0.0"

  fun version(versionName: String?): String {
    val found = if (versionName == null) null else LEADING.find(versionName)
    return if (found == null) NONE else "${found.groupValues[1]}.${found.groupValues[2]}.${found.groupValues[3]}"
  }

  fun of(versionName: String?): String = "$NAME${version(versionName)} (android)"

  /** The web view's own User-Agent with the token at the end; never twice. */
  fun appended(userAgent: String, versionName: String?): String {
    if (userAgent.contains(NAME)) return userAgent
    return "$userAgent ${of(versionName)}"
  }
}
