package online.dynodoc.client

import java.net.URI
import java.util.Locale

// Pure functions (a plain JVM unit test covers them): what "the same site" means. An origin is
// scheme, host and port, compared exactly (default ports filled in, scheme and host in lower
// case); never a comparison of host strings alone, which would treat http://host, https://host
// and https://host:8443 as one site.
object Origins {
  /** `scheme://host:port` of an http(s) address without a user name, else null. */
  fun of(url: String?): String? {
    if (url == null) return null
    val uri = try {
      URI(url)
    } catch (e: Exception) {
      return null
    }
    val scheme = uri.scheme?.lowercase(Locale.ROOT) ?: return null
    if (scheme != "http" && scheme != "https") return null
    if (uri.rawUserInfo != null) return null
    val host = uri.host?.lowercase(Locale.ROOT)
    if (host.isNullOrEmpty()) return null
    val port = if (uri.port == -1) (if (scheme == "https") 443 else 80) else uri.port
    return "$scheme://$host:$port"
  }

  /** True only if both are http(s) addresses of exactly one origin. */
  fun same(a: String?, b: String?): Boolean {
    val first = of(a) ?: return false
    return first == of(b)
  }
}
