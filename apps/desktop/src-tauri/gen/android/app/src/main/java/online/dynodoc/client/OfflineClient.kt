package online.dynodoc.client

import android.app.Activity
import android.graphics.Bitmap
import android.net.Uri
import android.text.Html
import android.util.Log
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import java.io.ByteArrayInputStream

// Wraps wry's web view client. Everything is forwarded to it (it is the one that asks Rust
// whether a page may load); this class adds four things around the edges and decides nothing
// about which links may load:
//  1. a plain page with a "Try again" link instead of Android's stock error page (no retry there);
//  2. a refusal of main-frame requests Android never reports to Rust: a form POST, or a history
//     step, to an origin other than the site's (shouldOverrideUrlLoading only sees GET links);
//  3. the launcher is dropped from the back/forward list once the site has loaded, so a page's
//     history.back() cannot bring it up;
//  4. a dead renderer re-creates the activity instead of leaving an unusable web view.
// It holds no script and has no way for a page to call it.
class OfflineClient(private val activity: Activity, private val inner: WebViewClient) : WebViewClient() {
  // The origin of the first page of the site that started loading (a page the shell had let
  // through). Null until then: while the launcher is showing nothing is refused.
  private var siteOrigin: String? = null

  override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
    if (request.isForMainFrame && refuses(request.url.toString())) {
      Log.w(TAG, "refused a main-frame ${request.method} request to ${Origins.of(request.url.toString())}")
      return refusal()
    }
    return inner.shouldInterceptRequest(view, request)
  }

  // The answer to a refused request: 403 with a short plain page (the web view shows an answer in
  // place of the page that asked, so an empty one would leave a blank window). Nothing leaves the
  // device. The one link goes back to the site's entry page, through the usual navigation rule.
  private fun refusal(): WebResourceResponse {
    val site = siteOrigin
    val link = if (site != null) "<p><a href=\"${Html.escapeHtml(site)}/workspace\">Back to Dynodoc</a></p>" else ""
    val html = """<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light dark">
<title>Dynodoc</title><style>
body{font:16px/1.5 system-ui,sans-serif;margin:0;padding:2rem 1rem;background:#f7f5ef;color:#1c2230}
@media (prefers-color-scheme:dark){body{background:#11151c;color:#e8ebf1}a{color:#8fb4ff}}
a{color:#2059b0;display:inline-block;min-height:44px;padding:.6rem 0;font-weight:600}
</style></head><body><h1>Dynodoc did not open this address</h1>
<p>A page asked to send you somewhere that is not your Dynodoc server.</p>$link</body></html>"""
    return WebResourceResponse(
      "text/html", "utf-8", 403, "Forbidden",
      mapOf("Cache-Control" to "no-store"), ByteArrayInputStream(html.toByteArray(Charsets.UTF_8))
    )
  }

  /** Only http(s) main-frame requests are judged here; the rest stays wry's and Rust's business. */
  private fun refuses(url: String): Boolean {
    val origin = Origins.of(url) ?: return false
    if (Uri.parse(url).host == LAUNCHER_HOST) return false
    val site = siteOrigin ?: return false
    return origin != site
  }

  @Deprecated("Deprecated in Java")
  override fun shouldOverrideUrlLoading(view: WebView, url: String): Boolean =
    @Suppress("DEPRECATION") inner.shouldOverrideUrlLoading(view, url)

  override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
    inner.shouldOverrideUrlLoading(view, request)

  override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
    if (siteOrigin == null && Uri.parse(url).host != LAUNCHER_HOST) siteOrigin = Origins.of(url)
    inner.onPageStarted(view, url, favicon)
  }

  override fun onPageFinished(view: WebView, url: String) {
    inner.onPageFinished(view, url)
    if (Uri.parse(url).host != LAUNCHER_HOST && Origins.of(url) != null) dropLauncherFromHistory(view)
  }

  private fun dropLauncherFromHistory(view: WebView) {
    val history = view.copyBackForwardList()
    for (i in 0 until history.size - 1) {
      if (Uri.parse(history.getItemAtIndex(i)?.url ?: "").host == LAUNCHER_HOST) {
        view.clearHistory()
        return
      }
    }
  }

  override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
    inner.onReceivedError(view, request, error)
    val url = request.url
    val web = url.scheme == "https" || url.scheme == "http"
    val network = error.errorCode == ERROR_HOST_LOOKUP || error.errorCode == ERROR_CONNECT ||
      error.errorCode == ERROR_TIMEOUT || error.errorCode == ERROR_IO
    if (request.isForMainFrame && web && network && url.host != LAUNCHER_HOST) {
      view.post { showOffline(view, url) }
    }
  }

  override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
    if (inner.onRenderProcessGone(view, detail)) return true
    // Without this the app is killed with its dead renderer. The web view cannot be used any
    // more: build a new one by re-creating the activity (wry makes the web view, Rust loads the
    // site's entry page again).
    Log.w(TAG, "the web view's renderer is gone (crashed: ${detail.didCrash()}); re-creating the activity")
    activity.runOnUiThread { activity.recreate() }
    return true
  }

  private fun showOffline(view: WebView, url: Uri) {
    val address = Html.escapeHtml(url.toString())
    val host = Html.escapeHtml(url.host ?: "")
    // No base address: nothing in this page is ever treated as a page of the site. The history
    // address is the failed one, so a reload also tries again.
    view.loadDataWithBaseURL(
      null,
      """<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light dark">
<title>Dynodoc</title><style>
body{font:16px/1.5 system-ui,sans-serif;margin:0;padding:2rem 1rem;background:#f7f5ef;color:#1c2230}
@media (prefers-color-scheme:dark){body{background:#11151c;color:#e8ebf1}a{color:#8fb4ff}}
a{color:#2059b0;display:inline-block;min-height:44px;padding:.6rem 0;font-weight:600}
</style></head><body><h1>Dynodoc cannot reach $host</h1>
<p>Check your connection, then try again.</p>
<p><a href="$address">Try again</a></p></body></html>""",
      "text/html",
      "utf-8",
      url.toString()
    )
  }

  companion object {
    const val TAG = "Dynodoc"
    private const val LAUNCHER_HOST = "tauri.localhost"

    // The calls wry's client overrides today (plus the one this class adds). If a later wry
    // overrides more, this wrapper would swallow them, so it is not installed (the stock error
    // page comes back and the extra refusals are lost, nothing else breaks).
    private val FORWARDED = setOf(
      "shouldInterceptRequest", "shouldOverrideUrlLoading", "onPageStarted", "onPageFinished",
      "onReceivedError", "onRenderProcessGone"
    )

    fun installOn(activity: Activity, webView: WebView) {
      val inner = webView.webViewClient
      val known = WebViewClient::class.java.methods.map { it.name }.toSet()
      val overridden = inner.javaClass.declaredMethods.map { it.name }.filter { it in known }.toSet()
      if (!FORWARDED.containsAll(overridden)) {
        Log.w(TAG, "wry's web view client overrides more than OfflineClient forwards: $overridden; not installed")
        return
      }
      webView.webViewClient = OfflineClient(activity, inner)
    }
  }
}
