// Reading a message the way a receiving program would, for tests: headers unfolded and
// RFC 2047 words decoded, MIME parts split and their transfer encodings (base64,
// quoted-printable) undone. Test only.

/** Split a built message into its header block and body. */
export function split(raw: Buffer) {
  const text = raw.toString('latin1');
  const end = text.indexOf('\r\n\r\n');
  const headers = new Map<string, string>();
  for (const line of text
    .slice(0, end)
    .replace(/\r\n[ \t]+/g, ' ')
    .split('\r\n')) {
    const colon = line.indexOf(':');
    headers.set(line.slice(0, colon).toLowerCase(), line.slice(colon + 1).trim());
  }
  return { headers, body: text.slice(end + 4) };
}
/** Decode RFC 2047 encoded words (B and Q) in a header value. */
export function decodeWords(value: string) {
  return value
    .replace(/\?=\s+=\?/g, '?==?')
    .replace(/=\?([^?]+)\?([BbQq])\?([^?]*)\?=/g, (_m, _charset, kind: string, data: string) =>
      kind.toUpperCase() === 'B'
        ? Buffer.from(data, 'base64').toString('utf8')
        : Buffer.from(
            data
              .replace(/_/g, ' ')
              .replace(/=([0-9A-F]{2})/gi, (_x, hex: string) =>
                String.fromCharCode(parseInt(hex, 16))
              ),
            'latin1'
          ).toString('utf8')
    );
}
/** Decode one MIME part's body according to its transfer encoding. */
export function decodePart(part: string) {
  const [head, ...rest] = part.split('\r\n\r\n');
  const body = rest.join('\r\n\r\n').replace(/\r\n$/, '');
  const encoding = /content-transfer-encoding:\s*(\S+)/i.exec(head)?.[1]?.toLowerCase();
  if (encoding === 'base64') return Buffer.from(body, 'base64').toString('utf8');
  if (encoding === 'quoted-printable')
    return Buffer.from(
      body
        .replace(/=\r\n/g, '')
        .replace(/=([0-9A-F]{2})/g, (_m, hex: string) => String.fromCharCode(parseInt(hex, 16))),
      'latin1'
    ).toString('utf8');
  return body;
}

/** The decoded text and HTML parts of a message as received (CRLF line ends become LF). */
export function readMessage(data: string) {
  const { headers, body } = split(Buffer.from(data, 'utf8'));
  const boundary = /boundary="?([^";]+)"?/.exec(headers.get('content-type') ?? '')?.[1];
  const decoded: Record<string, string> = {};
  const parts = boundary ? body.split(`--${boundary}`).slice(1, -1) : [`\r\n${body}`];
  for (const part of parts) {
    const text = part.replace(/^\r\n/, '');
    const type = /content-type:\s*([^;\r\n]+)/i.exec(text)?.[1]?.toLowerCase() ?? 'text/plain';
    decoded[type] = decodePart(text).replace(/\r\n/g, '\n');
  }
  return {
    headers,
    subject: decodeWords(headers.get('subject') ?? ''),
    text: decoded['text/plain'] ?? '',
    html: decoded['text/html']
  };
}
