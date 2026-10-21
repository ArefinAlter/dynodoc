/**
 * Copy text with a fallback. The async clipboard API is missing on pages that are not
 * served securely and can be refused (Safari wants a direct tap, some embedded browsers
 * never allow it), so fall back to selecting a hidden field and the older copy command.
 * Resolves false if both fail; callers show the text so it can be copied by hand.
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    /* fall through to the older command */
  }
  const field = document.createElement('textarea');
  field.value = text;
  field.setAttribute('readonly', '');
  field.setAttribute('aria-hidden', 'true');
  // Off screen, and 16px so iOS does not zoom the page while it is selected.
  field.style.cssText = 'position:fixed;top:0;left:-9999px;opacity:0;font-size:16px';
  document.body.append(field);
  field.select();
  field.setSelectionRange(0, text.length);
  try {
    return document.execCommand('copy');
  } catch {
    return false;
  } finally {
    field.remove();
  }
}
