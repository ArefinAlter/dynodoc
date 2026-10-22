package online.dynodoc.client

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AppTokenTest {
  private val base =
    "Mozilla/5.0 (Linux; Android 16; Pixel 6 Build/BP1A.250305.019; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/133.0.0.0 Mobile Safari/537.36"

  /** The cases every language runs (apps/desktop/fixtures/app-token.json, one case per line). */
  private fun fixture(): List<Pair<String, String>> {
    // Gradle runs unit tests with the module (gen/android/app) as the working directory.
    val file = File("../../../../fixtures/app-token.json")
    assertTrue("fixture not found at ${file.absolutePath}", file.exists())
    val line = Regex("""\{ "raw": "(.*)", "version": "(.*)" \}""")
    return file.readLines(Charsets.UTF_8).mapNotNull { line.find(it)?.let { m -> m.groupValues[1] to m.groupValues[2] } }
  }

  @Test
  fun theTokenNamesTheVersionAndTheKind() {
    assertEquals("DynodocApp/0.1.0 (android)", AppToken.of("0.1.0"))
    assertEquals("DynodocApp/10.20.300 (android)", AppToken.of("10.20.300"))
    assertEquals("DynodocApp/2026.10.22 (android)", AppToken.of("2026.10.22"))
  }

  @Test
  fun theVersionFollowsTheOneGrammarTheFixturesPin() {
    val cases = fixture()
    assertTrue("only ${cases.size} fixture cases read", cases.size > 15)
    for ((raw, version) in cases) assertEquals(raw, version, AppToken.version(raw))
    assertEquals("0.0.0", AppToken.version(null))
    // The two long cases every language generates for itself.
    assertEquals("1.2.3", AppToken.version("1.2.3-" + "x".repeat(5000)))
    assertEquals("0.0.0", AppToken.version("9".repeat(400) + ".0.0"))
  }

  @Test
  fun aVersionThatIsNotThreeNumbersBecomesZero() {
    for (bad in listOf(null, "", "1", "1.2", "v1.2.3", "1.2.x", " 1.2.3", "1234567890.0.0", "x1.2.3", "1.2.3".replace("1", "١"))) {
      assertEquals(bad, "DynodocApp/0.0.0 (android)", AppToken.of(bad))
    }
  }

  @Test
  fun aSuffixIsDroppedOnPurposeSoTheServerNeverRefusesTheToken() {
    assertEquals("DynodocApp/0.1.0 (android)", AppToken.of("0.1.0-beta.1"))
    assertEquals("DynodocApp/10.0.0 (android)", AppToken.of("10.0.0+build5"))
    assertEquals("DynodocApp/1.2.3 (android)", AppToken.of("1.2.3.4"))
  }

  @Test
  fun theTokenIsAppendedToTheWebViewsOwnAgentOnce() {
    val once = AppToken.appended(base, "0.1.0")
    assertEquals("$base DynodocApp/0.1.0 (android)", once)
    assertTrue(once.startsWith(base))
    // The activity is created again after the system re-creates it: never a second token.
    assertEquals(once, AppToken.appended(once, "0.1.0"))
  }
}
