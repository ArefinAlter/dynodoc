package online.dynodoc.client

import android.app.DownloadManager
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.os.SystemClock
import android.util.Log
import android.view.MotionEvent
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
// happens to a download. Which pages may load is decided in Rust (nav.rs); OfflineClient adds the
// few navigations Android never reports to Rust.
class MainActivity : TauriActivity() {
  // elapsedRealtime of the last touch the web view really received (obscured touches are
  // filtered out before it sees them) and of the last download started.
  private var lastTouch = Long.MIN_VALUE / 2
  private var lastDownload = Long.MIN_VALUE / 2

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    if (savedInstanceState != null) {
      // The system re-created this activity (font size, process death, a dead renderer). Its
      // launch intent is still the one that started it, and the deep-link plugin would replay a
      // dynodoc:// link from it and pull the window back to that page. Replace it.
      intent = Intent(this, MainActivity::class.java).setAction(Intent.ACTION_MAIN)
    }
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

  // The activity handles a light/dark switch itself (the web view must not reload), so the
  // system bars' icon colours and the colour behind the page are updated here.
  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    enableEdgeToEdge()
    window.setBackgroundDrawableResource(R.color.dynodoc_window)
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
    // Touches that arrive while another window covers part of this one are dropped (tapjacking).
    webView.filterTouchesWhenObscured = true
    webView.setOnTouchListener { _, event ->
      if (event.actionMasked == MotionEvent.ACTION_DOWN) lastTouch = SystemClock.elapsedRealtime()
      false
    }
    // wry calls this BEFORE it sets its own web view client, so wrap that client once the rest
    // of wry's set-up (same call, same thread) has finished.
    webView.post { OfflineClient.installOn(this, webView) }
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
    return Origins.same(here, before)
  }

  private fun say(message: String, length: Int = Toast.LENGTH_LONG) {
    Toast.makeText(this, message, length).show()
  }

  // A web view ignores a download unless the app handles it. This handles the narrowest case and
  // refuses the rest, because a page's script can start downloads too and the name, the type
  // and the redirects of an answer all come from outside:
  //  - only after a touch of the web view in the last 5 seconds, and one download per 3 seconds;
  //  - only a plain https file of exactly the origin of the page shown (scheme, host and port);
  //  - NO cookies are sent (the download manager would repeat them on every redirect, to any host),
  //    so a download the site would only give to a signed-in person is not supported here: if the
  //    cookie jar holds cookies for the address, the person is told to use the browser;
  //  - the file name is cleaned and anything installable or runnable is refused (DownloadNames);
  //  - a blob: or data: address exists only inside the page and cannot be fetched from here (that
  //    needs a script bridge, which this app does not have): the person is told.
  // The download manager saves into Download/ without any storage permission on Android 10 and later
  // (minSdkVersion is 29 for that reason).
  private fun startDownload(webView: WebView, url: String, contentDisposition: String?, mimeType: String?) {
    val now = SystemClock.elapsedRealtime()
    if (now - lastTouch > 5000) {
      Log.w(TAG, "download refused: no touch of the page in the last 5 seconds")
      return
    }
    if (now - lastDownload < 3000) {
      Log.w(TAG, "download refused: one per 3 seconds")
      return
    }
    val page = webView.url
    if (Origins.of(url)?.startsWith("https://") != true || !Origins.same(url, page)) {
      Log.w(TAG, "download refused: not an https file of the page's own origin")
      say("Not supported in the app yet. Download this in your web browser.")
      return
    }
    if (!CookieManager.getInstance().getCookie(url).isNullOrEmpty()) {
      Log.w(TAG, "download refused: the site has cookies set (a signed-in download needs them, and they are never sent)")
      say("Not supported in the app yet. Download this in your web browser.")
      return
    }
    val name = DownloadNames.safe(URLUtil.guessFileName(url, contentDisposition, mimeType), mimeType)
    if (name == null) {
      Log.w(TAG, "download refused: an installable or runnable file")
      say("This kind of file is not downloaded by the app.")
      return
    }
    try {
      val request = DownloadManager.Request(Uri.parse(url))
        .setTitle(name)
        .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
        .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, name)
      if (!mimeType.isNullOrEmpty()) request.setMimeType(mimeType)
      (getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager).enqueue(request)
      lastDownload = now
      Log.i(TAG, "download enqueued")
      say("Downloading $name", Toast.LENGTH_SHORT)
    } catch (e: Exception) {
      Log.w(TAG, "download could not be started: ${e.javaClass.simpleName}")
      say("The download could not be started.")
    }
  }

  companion object {
    const val TAG = "Dynodoc"
  }
}
