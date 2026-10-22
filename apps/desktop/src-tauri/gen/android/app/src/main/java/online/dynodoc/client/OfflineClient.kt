package online.dynodoc.client

import android.graphics.Bitmap
import android.net.Uri
import android.text.Html
import android.util.Log
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient

// Replaces the web view's stock "Webpage not available" error page, which has no way to try
// again, with a short page that has one. This class forwards every call to wry's own client
// (the one that asks Rust whether a page may load) and adds the page on a failed load of the
// site; it decides nothing about which pages may load. It holds no script: the page is plain
// HTML with one link back to the address that failed, and that link goes through Rust's
// navigation rule like any other.
class OfflineClient(private val inner: WebViewClient) : WebViewClient() {
  override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
    inner.shouldInterceptRequest(view, request)

  @Deprecated("Deprecated in Java")
  override fun shouldOverrideUrlLoading(view: WebView, url: String): Boolean =
    @Suppress("DEPRECATION") inner.shouldOverrideUrlLoading(view, url)

  override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
    inner.shouldOverrideUrlLoading(view, request)

  override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) =
    inner.onPageStarted(view, url, favicon)

  override fun onPageFinished(view: WebView, url: String) = inner.onPageFinished(view, url)

  override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
    inner.onReceivedError(view, request, error)
    val url = request.url
    val web = url.scheme == "https" || url.scheme == "http"
    val network = error.errorCode == ERROR_HOST_LOOKUP || error.errorCode == ERROR_CONNECT ||
      error.errorCode == ERROR_TIMEOUT || error.errorCode == ERROR_IO
    if (request.isForMainFrame && web && network && url.host != "tauri.localhost") {
      view.post { showOffline(view, url) }
    }
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
    // The calls wry's client overrides today. If a later wry overrides more, this wrapper would
    // swallow them, so it is not installed (the stock error page comes back, nothing breaks).
    private val FORWARDED = setOf(
      "shouldInterceptRequest", "shouldOverrideUrlLoading", "onPageStarted", "onPageFinished", "onReceivedError"
    )

    fun installOn(webView: WebView) {
      if (android.os.Build.VERSION.SDK_INT < 26) return
      val inner = webView.webViewClient
      val known = WebViewClient::class.java.methods.map { it.name }.toSet()
      val overridden = inner.javaClass.declaredMethods.map { it.name }.filter { it in known }.toSet()
      if (!FORWARDED.containsAll(overridden)) {
        Log.w("Dynodoc", "wry's web view client overrides more than OfflineClient forwards: $overridden; not installed")
        return
      }
      webView.webViewClient = OfflineClient(inner)
    }
  }
}
