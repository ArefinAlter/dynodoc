// A throwaway SMTP sink for the bundle's smoke test. TESTS ONLY: it is not part of the product and
// is never started by the product's compose.yml. It accepts any message (no TLS, no login) and
// keeps it in memory; the test reads the messages over HTTP on the container's own loopback.
//
//   SMTP   0.0.0.0:2525   (the web container sends here with SMTP_SECURITY=none-local)
//   HTTP   127.0.0.1:2580  GET /messages -> [{ from, to: [...], data }]   DELETE /messages
import net from 'node:net';
import http from 'node:http';

const messages = [];

net
  .createServer((socket) => {
    let buffer = '';
    let data = null;
    let from = '';
    let to = [];
    socket.setEncoding('utf8');
    const reply = (line) => socket.write(`${line}\r\n`);
    reply('220 sink.test ESMTP dynodoc-smoke-sink');
    socket.on('data', (chunk) => {
      buffer += chunk;
      for (;;) {
        if (data !== null) {
          // The end marker may be split across two chunks, so look for it in everything so far.
          const empty = buffer.startsWith('.\r\n');
          const end = empty ? 0 : buffer.indexOf('\r\n.\r\n');
          if (end < 0) return;
          messages.push({ from, to, data: empty ? '' : buffer.slice(0, end) });
          buffer = buffer.slice(empty ? 3 : end + 5);
          data = null;
          reply('250 2.0.0 queued');
          continue;
        }
        const eol = buffer.indexOf('\r\n');
        if (eol < 0) return;
        const line = buffer.slice(0, eol);
        buffer = buffer.slice(eol + 2);
        const verb = line.slice(0, 4).toUpperCase();
        if (verb === 'EHLO' || verb === 'HELO') reply('250-sink.test\r\n250 8BITMIME');
        else if (verb === 'MAIL') {
          from = /<([^>]*)>/.exec(line)?.[1] ?? '';
          to = [];
          reply('250 2.1.0 ok');
        } else if (verb === 'RCPT') {
          to.push(/<([^>]*)>/.exec(line)?.[1] ?? '');
          reply('250 2.1.5 ok');
        } else if (verb === 'DATA') {
          data = '';
          reply('354 end with <CRLF>.<CRLF>');
        } else if (verb === 'QUIT') {
          reply('221 bye');
          socket.end();
          return;
        } else if (verb === 'RSET' || verb === 'NOOP') reply('250 ok');
        else reply('502 5.5.2 not implemented');
      }
    });
    socket.on('error', () => {});
  })
  .listen(2525, '0.0.0.0');

http
  .createServer((request, response) => {
    if (request.url === '/messages' && request.method === 'DELETE') messages.length = 0;
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify(messages));
  })
  .listen(2580, '127.0.0.1');
