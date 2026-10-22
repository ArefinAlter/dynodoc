package online.dynodoc.client

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AppTokenTest {
  private val base =
    "Mozilla/5.0 (Linux; Android 16; Pixel 6 Build/BP1A.250305.019; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/133.0.0.0 Mobile Safari/537.36"

  @Test
  fun theTokenNamesTheVersionAndTheKind() {
    assertEquals("DynodocApp/0.1.0 (android)", AppToken.of("0.1.0"))
    assertEquals("DynodocApp/10.20.300 (android)", AppToken.of("10.20.300"))
  }

  @Test
  fun aVersionThatIsNotThreeNumbersBecomesZero() {
    for (bad in listOf(null, "", "1", "1.2", "1.2.3.4", "1.2.3-beta", "v1.2.3", "1.2.x", " 1.2.3", "1.2.3 ", "1234567.0.0", "1.2.3) x (")) {
      assertEquals(bad, "DynodocApp/0.0.0 (android)", AppToken.of(bad))
    }
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
