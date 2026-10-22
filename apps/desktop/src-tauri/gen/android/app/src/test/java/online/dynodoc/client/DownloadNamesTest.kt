package online.dynodoc.client

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// Plain JVM tests (./gradlew testUniversalDebugUnitTest): DownloadNames uses no Android class.
class DownloadNamesTest {
  @Test
  fun ordinaryNamesPass() {
    assertEquals("report.pdf", DownloadNames.safe("report.pdf", "application/pdf"))
    assertEquals("robots.txt", DownloadNames.safe("robots.txt", null))
    assertEquals("Jahresbericht 2026.xlsx", DownloadNames.safe("Jahresbericht 2026.xlsx", "application/octet-stream"))
    assertEquals("বাংলা.pdf", DownloadNames.safe("বাংলা.pdf", "application/pdf"))
  }

  @Test
  fun separatorsAndTraversalLoseTheirDirectory() {
    assertEquals("x.pdf", DownloadNames.safe("../../x.pdf", null))
    assertEquals("x.pdf", DownloadNames.safe("a\\b\\x.pdf", null))
    assertEquals("x.pdf", DownloadNames.safe("/etc/x.pdf", null))
  }

  @Test
  fun hiddenAndDotNamesAreNotHidden() {
    assertEquals("hidden", DownloadNames.safe(".hidden", null))
    assertEquals("hidden.txt", DownloadNames.safe("...hidden.txt", null))
    assertEquals("download", DownloadNames.safe("...", null))
    assertEquals("download", DownloadNames.safe("", null))
    assertEquals("download", DownloadNames.safe(null, null))
    assertEquals("download", DownloadNames.safe("  ", null))
  }

  @Test
  fun controlAndBidirectionalCharactersAreDropped() {
    assertEquals("ab.pdf", DownloadNames.safe("a\u0000b\n.pdf", null))
    // U+202E makes "invoice-exe.txt" read as "invoice-txt.exe": the override is dropped and the name
    // is judged as it really is.
    assertEquals("invoice-gpj.txt", DownloadNames.safe("invoice-‮gpj.txt", null))
    assertNull(DownloadNames.safe("invoice-‮txt.exe", null))
    assertEquals("ab.txt", DownloadNames.safe("a​b.txt", null))
    assertEquals("ab.txt", DownloadNames.safe("a⁦b⁩.txt", null))
  }

  @Test
  fun filesAPhoneCouldInstallOrRunAreRefusedWhateverTheTypeSays() {
    for (name in listOf("x.apk", "x.APK", "x.Apk", "x.apk.", "x.apk ", "x.xapk", "x.apks", "x.apkm", "x.aab", "x.dex", "x.jar", "x.sh", "photo.jpg.apk", "x.apk.txt", "doc.exe")) {
      assertNull(name, DownloadNames.safe(name, "image/jpeg"))
    }
    // The other way round: a harmless name with an installable type.
    assertNull(DownloadNames.safe("holiday.jpg", "application/vnd.android.package-archive"))
    assertNull(DownloadNames.safe("holiday.jpg", "Application/Vnd.Android.Package-Archive; charset=binary"))
    assertNull(DownloadNames.safe("holiday.jpg", "application/java-archive"))
    // "apk" as a plain word is fine.
    assertEquals("apk", DownloadNames.safe("apk", null))
    assertEquals("apk.txt", DownloadNames.safe("apk.txt", null))
  }

  @Test
  fun lengthIsBoundedAndTheExtensionKept() {
    val long = DownloadNames.safe("a".repeat(300) + ".pdf", null)!!
    assertEquals(100, long.length)
    assertTrue(long.endsWith(".pdf"))
    assertEquals(100, DownloadNames.safe("b".repeat(500), null)!!.length)
  }

  @Test
  fun charactersAFileSystemTreatsSpeciallyAreReplaced() {
    assertEquals("a_b_c_d_e_f_g.txt", DownloadNames.safe("a<b>c:d\"e|f?g.txt", null))
    assertEquals("a_b.txt", DownloadNames.safe("a*b.txt", null))
  }
}
