package online.dynodoc.client

import java.util.Locale

// Pure functions (no Android classes, so a plain JVM unit test covers them) that decide what a
// download may be called and whether it may be saved at all. The name and the type come from the
// server's answer, so both are treated as input from outside.
object DownloadNames {
  private const val MAX_NAME = 100

  /** Files a phone could install or run. Refused whatever the type of the answer claims. */
  private val REFUSED_EXTENSIONS = setOf(
    "apk", "xapk", "apks", "apkm", "aab", "dex", "odex", "oat", "vdex", "jar", "so", "elf",
    "sh", "bat", "cmd", "exe", "msi", "scr", "dll", "vbs", "ps1", "lnk", "jnlp", "hta"
  )

  /** Types that mean "an installable package". */
  private val REFUSED_TYPES = setOf(
    "application/vnd.android.package-archive",
    "application/x-android-package",
    "application/vnd.android.package",
    "application/x-apkg",
    "application/java-archive",
    "application/x-msdownload",
    "application/x-msdos-program",
    "application/x-sh",
    "application/x-dex",
  )

  /**
   * A file name that is safe to hand to the download manager, or null if this download must not
   * be saved: control and invisible characters (including the bidirectional overrides that make
   * `exe.txt` read as `txt.exe`) are dropped, separators and characters a file system treats
   * specially become "_", leading dots and spaces go (no hidden files), the length is bounded
   * with the extension kept, and a name or a type that means an installable or runnable file is
   * refused even when the other one looks harmless.
   */
  fun safe(raw: String?, mimeType: String?): String? {
    val type = mimeType?.substringBefore(';')?.trim()?.lowercase(Locale.ROOT).orEmpty()
    if (type in REFUSED_TYPES) return null
    val last = (raw ?: "").substringAfterLast('/').substringAfterLast('\\')
    val cleaned = StringBuilder()
    for (ch in last) {
      val kind = Character.getType(ch)
      when {
        Character.isISOControl(ch) -> {}
        kind == Character.FORMAT.toInt() -> {} // bidi overrides, zero-width characters, soft hyphen
        kind == Character.LINE_SEPARATOR.toInt() || kind == Character.PARAGRAPH_SEPARATOR.toInt() -> {}
        kind == Character.UNASSIGNED.toInt() || kind == Character.PRIVATE_USE.toInt() || kind == Character.SURROGATE.toInt() && !isPairedSurrogate(last, ch) -> {}
        ch in "<>:\"|?*" -> cleaned.append('_')
        ch == ' ' || ch == ' ' -> cleaned.append(' ')
        else -> cleaned.append(ch)
      }
    }
    // No hidden files, no leading or trailing dots and spaces (".apk", "x.apk." and "x.apk " all
    // mean an apk to somebody).
    var name = cleaned.toString().trim().trimStart('.').trim().trimEnd('.', ' ')
    if (name.isEmpty()) name = "download"
    if (name.length > MAX_NAME) {
      val dot = name.lastIndexOf('.')
      val ext = if (dot > 0 && name.length - dot <= 12) name.substring(dot) else ""
      name = name.substring(0, MAX_NAME - ext.length) + ext
    }
    // Every dot-separated piece after the first is looked at, not only the last one: "a.apk.txt"
    // is refused too, because a viewer that goes by the middle part would offer to install it.
    val pieces = name.lowercase(Locale.ROOT).split('.')
    if (pieces.drop(1).any { it.trim() in REFUSED_EXTENSIONS }) return null
    return name
  }

  private fun isPairedSurrogate(text: String, ch: Char): Boolean {
    val at = text.indexOf(ch)
    return if (Character.isHighSurrogate(ch)) at + 1 < text.length && Character.isLowSurrogate(text[at + 1])
    else at > 0 && Character.isHighSurrogate(text[at - 1])
  }
}
