package online.dynodoc.client

// Pure functions (a plain JVM unit test covers them): the token this app adds to its web view's own
// User-Agent, `DynodocApp/<major.minor.patch> (android)`. The server's app sign-in finish page and
// its sign-in routes work only for a window that carries it (docs/decisions/017). It is the same
// shape the web tier reads (`appKind` in frontend/researcher/src/lib/app-signin.ts); a version name
// that is not three numbers (a pre-release suffix, a build tag) is written as 0.0.0 rather than
// leaving the token malformed and the sign-in refused.
object AppToken {
  private val VERSION = Regex("""\d{1,6}\.\d{1,6}\.\d{1,6}""")
  private const val NAME = "DynodocApp/"

  fun of(versionName: String?): String {
    val version = if (versionName != null && VERSION.matches(versionName)) versionName else "0.0.0"
    return "$NAME$version (android)"
  }

  /** The web view's own User-Agent with the token at the end; never twice. */
  fun appended(userAgent: String, versionName: String?): String {
    if (userAgent.contains(NAME)) return userAgent
    return "$userAgent ${of(versionName)}"
  }
}
