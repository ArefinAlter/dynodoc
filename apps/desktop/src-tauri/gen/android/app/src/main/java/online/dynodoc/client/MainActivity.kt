package online.dynodoc.client

import android.app.DownloadManager
import android.content.Context
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.view.View
import android.webkit.CookieManager
import android.webkit.URLUtil
import android.webkit.WebSettings
import android.webkit.WebView
import android.widget.Toast
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

// What this app adds to Tauri's activity. There is NO JavaScript bridge here: no
// addJavascriptInterface, no page-callable function. A page of the server cannot reach any of
// this code; it only changes how the web view sits in the window, how Back behaves and what
// happens to a download. Everything about which pages may load is decided in Rust (nav.rs).
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Android draws apps edge to edge (target SDK 35+) and the web view reports no safe area, so
    // keep the page out from under the status bar, the navigation bar, a camera cut-out and the
    // soft keyboard (otherwise a form field at the bottom hides behind the keyboard).
    ViewCompat.setOnApplyWindowInsetsListener(findViewById<View>(android.R.id.content)) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or
          WindowInsetsCompat.Type.displayCutout() or
          WindowInsetsCompat.Type.ime()
      )
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      WindowInsetsCompat.CONSUMED
    }
  }

  // Called by wry when the web view exists (again after the system re-creates the activity).
  override fun onWebViewCreate(webView: WebView) {
    with(webView.settings) {
      // Stated, not left to the target SDK's defaults: a page of the server never needs the
      // device's files, and never may load http content into an https page.
      allowFileAccess = false
      allowContentAccess = false
      mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
      // No second window: a link with target=_blank or window.open loads in this one web view,
      // where Rust decides whether it may (the server's pages) or goes to the system browser.
      // window.open without a tap on the page is refused by the web view itself.
      setSupportMultipleWindows(false)
      javaScriptCanOpenWindowsAutomatically = false
    }
    // wry calls this BEFORE it sets its own web view client, so wrap that client once the rest
    // of wry's set-up (same call, same thread) has finished.
    webView.post { OfflineClient.installOn(webView) }
    webView.setDownloadListener { url, _, contentDisposition, mimeType, _ ->
      startDownload(webView, url, contentDisposition, mimeType)
    }
    // Tauri's activity leaves Back to the system, which would close the app from any page of
    // the server. Back goes back through the pages of the site shown; at the first page (or when
    // the page before is another site, such as the launcher this app started on) it does what a
    // root screen does in any app on Android 12 and later: the app goes to the background, with
    // its page kept, instead of being destroyed.
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        if (webView.canGoBack() && previousPageIsSameSite(webView)) {
          webView.goBack()
        } else if (!moveTaskToBack(true)) {
          finish()
        }
      }
    })
  }

  private fun previousPageIsSameSite(webView: WebView): Boolean {
    val history = webView.copyBackForwardList()
    val here = history.currentItem?.url ?: return false
    val before = history.getItemAtIndex(history.currentIndex - 1)?.url ?: return false
    val host = Uri.parse(here).host
    return host != null && host == Uri.parse(before).host
  }

  // A web view ignores a download unless the app handles it. Only a plain https file from the
  // very site being shown is handed to the system's download manager (with the site's cookies,
  // so a signed-in download works). A blob: or data: address exists only inside the page and
  // cannot be fetched from here (that needs a script bridge, which this app does not have), so
  // the person is told instead of being left with nothing.
  private fun startDownload(webView: WebView, url: String, contentDisposition: String?, mimeType: String?) {
    val file = Uri.parse(url)
    val page = Uri.parse(webView.url ?: "")
    val samePlainHttps = file.scheme == "https" && page.scheme == "https" &&
      !file.host.isNullOrEmpty() && file.host == page.host
    if (!samePlainHttps) {
      Toast.makeText(
        this,
        "This download is not supported in the Dynodoc app yet. Open Dynodoc in your web browser to download it.",
        Toast.LENGTH_LONG
      ).show()
      return
    }
    try {
      val name = URLUtil.guessFileName(url, contentDisposition, mimeType).replace('/', '_')
      val request = DownloadManager.Request(file)
        .setTitle(name)
        .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
        .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, name)
      if (!mimeType.isNullOrEmpty()) request.setMimeType(mimeType)
      CookieManager.getInstance().getCookie(url)?.let { request.addRequestHeader("Cookie", it) }
      (getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager).enqueue(request)
      Toast.makeText(this, "Downloading $name", Toast.LENGTH_SHORT).show()
    } catch (e: Exception) {
      Toast.makeText(this, "The download could not be started.", Toast.LENGTH_LONG).show()
    }
  }
}
