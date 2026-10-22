package online.dynodoc.client

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class OriginsTest {
  @Test
  fun anOriginIsSchemeHostAndPort() {
    assertEquals("https://app.dynodoc.online:443", Origins.of("https://app.dynodoc.online/workspace?x=1#y"))
    assertEquals("https://app.dynodoc.online:443", Origins.of("HTTPS://APP.Dynodoc.Online:443/"))
    assertEquals("http://localhost:5173", Origins.of("http://localhost:5173/a"))
    assertEquals("https://[2001:db8::1]:8443", Origins.of("https://[2001:db8::1]:8443/"))
  }

  @Test
  fun thingsThatAreNotAnHttpOriginHaveNone() {
    for (url in listOf(null, "", "about:blank", "data:text/html,x", "blob:https://a.example/1", "file:///x", "javascript:1", "mailto:a@b", "https://", "not a url", "https://user@a.example/", "https://a.example@b.example/")) {
      assertNull(url, Origins.of(url))
    }
  }

  @Test
  fun sameMeansExactlyTheSameOrigin() {
    assertTrue(Origins.same("https://a.example/x", "https://a.example/y"))
    assertTrue(Origins.same("https://A.example:443/x", "https://a.example/y"))
    assertFalse(Origins.same("https://a.example/", "http://a.example/"))
    assertFalse(Origins.same("https://a.example/", "https://a.example:8443/"))
    assertFalse(Origins.same("https://a.example/", "https://b.example/"))
    assertFalse(Origins.same("https://a.example/", "https://sub.a.example/"))
    assertFalse(Origins.same("https://a.example/", "https://a.example.evil.example/"))
    assertFalse(Origins.same("https://a.example/", "https://a.example@evil.example/"))
    assertFalse(Origins.same("https://a.example./", "https://a.example/"))
    assertFalse(Origins.same(null, null))
    assertFalse(Origins.same("about:blank", "about:blank"))
  }
}
