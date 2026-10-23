// Safe-by-default XML text. `xml` is a template tag that escapes every `${value}` placed in a
// template, so a manifest can only contain a value that went through `escapeXml` (or one the
// author explicitly wrapped with `raw`, which the manifest template never does for input).
// Plain JavaScript with no framework imports.

const ENTITIES = /** @type {Record<string, string>} */ ({
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&apos;'
});

/**
 * Escape text for an element or a double- or single-quoted attribute. Refuses what XML 1.0
 * cannot carry at all (control characters, unpaired surrogates, U+FFFE and U+FFFF) rather than
 * writing a document no parser accepts.
 * @param {unknown} value
 */
export function escapeXml(value) {
  const text = String(value);
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    const control = code < 0x20 && code !== 9 && code !== 10 && code !== 13;
    const lone = code >= 0xd800 && code <= 0xdfff;
    if (control || lone || code === 0xfffe || code === 0xffff)
      throw new RangeError('The text contains a character that XML cannot hold.');
  }
  return text.replace(/[&<>"']/g, (char) => ENTITIES[char]);
}

class Raw {
  /** @param {string} text */
  constructor(text) {
    this.text = text;
  }
}
/** Text that is already markup. Never pass input to this. @param {string} text */
export const raw = (text) => new Raw(text);

/**
 * @param {TemplateStringsArray} strings
 * @param {...unknown} values
 */
export function xml(strings, ...values) {
  let out = strings[0];
  values.forEach((value, index) => {
    out += (value instanceof Raw ? value.text : escapeXml(value)) + strings[index + 1];
  });
  return out;
}
