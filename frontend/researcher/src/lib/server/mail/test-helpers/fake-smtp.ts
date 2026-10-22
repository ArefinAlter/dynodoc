// An SMTP server in this process, for tests and for the end-to-end mail spec. It speaks enough
// of RFC 5321 to be talked to by a real client (EHLO, STARTTLS, AUTH PLAIN / LOGIN, MAIL, RCPT,
// DATA, RSET, QUIT), records everything it is sent, and can be told to misbehave: refuse,
// answer 4xx once, stay silent, offer AUTH only before TLS, leave out STARTTLS. Test only.
import net from 'node:net';
import tls from 'node:tls';

export interface FakeSmtpOptions {
  /** A certificate and key enable TLS: STARTTLS on a plain port, or TLS at once when implicit. */
  tls?: { key: string; cert: string; maxVersion?: tls.SecureVersion };
  /** TLS from the first byte, as on port 465. */
  implicitTls?: boolean;
  /** Offer STARTTLS on a plain connection. Default true when `tls` is given. */
  starttls?: boolean;
  /** Mechanisms offered once the connection is allowed to authenticate. Default none. */
  authMethods?: ('PLAIN' | 'LOGIN')[];
  /** Offer AUTH in the EHLO answer given before TLS too (a hostile or careless server). */
  authBeforeTls?: boolean;
  /** Accept AUTH before TLS when offered; otherwise answer 538. Default false. */
  acceptAuthBeforeTls?: boolean;
  credentials?: { user: string; pass: string };
  /** Answer MAIL FROM with 530 until the client has authenticated. */
  requireAuth?: boolean;
  /** Wait before the greeting; Infinity sends none at all. */
  greetingDelayMs?: number;
  /** Wait before every other reply. */
  replyDelayMs?: number;
  maxSize?: number;
  hostname?: string;
  /**
   * Replace a reply. `connection` counts connections (1-based), `message` the messages that
   * reached this step on this connection. Return undefined for the default reply.
   */
  reply?: Partial<
    Record<
      'mail' | 'rcpt' | 'data' | 'end',
      (info: { connection: number; message: number; line: string }) => string | undefined
    >
  >;
}

export interface ReceivedMessage {
  from: string;
  to: string[];
  /** The message as received, dot-unstuffed, CRLF line ends. */
  data: string;
  authUser: string | null;
  secure: boolean;
}

export interface FakeSmtp {
  host: string;
  port: number;
  messages: ReceivedMessage[];
  /** Every command line the client sent, and whether the connection was encrypted then. */
  lines: { text: string; secure: boolean }[];
  connections: number;
  /** What the client sent while the connection was not encrypted, joined. */
  plaintext(): string;
  close(): Promise<void>;
}

const b64 = (text: string) => Buffer.from(text).toString('base64');
const unb64 = (text: string) => Buffer.from(text, 'base64').toString('utf8');

export async function startFakeSmtp(options: FakeSmtpOptions = {}): Promise<FakeSmtp> {
  const hostname = options.hostname ?? 'mail.test.example';
  const state: FakeSmtp = {
    host: '127.0.0.1',
    port: 0,
    messages: [],
    lines: [],
    connections: 0,
    plaintext: () =>
      state.lines
        .filter((line) => !line.secure)
        .map((line) => line.text)
        .join('\n'),
    close: async () => {
      for (const socket of sockets) socket.destroy();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  };
  const sockets = new Set<net.Socket>();
  const context = options.tls
    ? tls.createSecureContext({
        key: options.tls.key,
        cert: options.tls.cert,
        maxVersion: options.tls.maxVersion
      })
    : undefined;

  const onConnection = (first: net.Socket, startsSecure: boolean) => {
    const connection = ++state.connections;
    sockets.add(first);
    first.on('close', () => sockets.delete(first));
    first.on('error', () => undefined);
    let socket: net.Socket = first;
    let secure = startsSecure;
    let authUser: string | null = null;
    let mailFrom = '';
    let mailOk = false;
    let rcpt: string[] = [];
    let dataMode = false;
    let buffer = '';
    let loginStep: 'user' | 'pass' | 'plain' | null = null;
    let loginUser = '';
    let delivered = { mail: 0, rcpt: 0, data: 0, end: 0 };
    let closing = false;

    const send = (text: string) => {
      const write = () => {
        if (!socket.destroyed) socket.write(`${text}\r\n`);
      };
      if (options.replyDelayMs) setTimeout(write, options.replyDelayMs);
      else write();
    };
    const scripted = (step: 'mail' | 'rcpt' | 'data' | 'end', line: string, fallback: string) => {
      delivered[step]++;
      return (
        options.reply?.[step]?.({ connection, message: delivered[step], line }) ?? fallback
      );
    };
    const mayAuthenticate = () =>
      (options.authMethods?.length ?? 0) > 0 &&
      (secure || !context || options.acceptAuthBeforeTls === true);
    const capabilities = () => {
      const lines = [`${hostname} greets you`, '8BITMIME', 'PIPELINING'];
      if (options.maxSize) lines.push(`SIZE ${options.maxSize}`);
      if (!secure && context && options.starttls !== false && !options.implicitTls)
        lines.push('STARTTLS');
      if (options.authMethods?.length && (secure || !context || options.authBeforeTls))
        lines.push(`AUTH ${options.authMethods.join(' ')}`);
      return lines;
    };

    const finishAuth = (user: string, pass: string) => {
      const ok = !options.credentials || (user === options.credentials.user && pass === options.credentials.pass);
      if (ok) {
        authUser = user;
        send('235 2.7.0 Authentication successful');
      } else send('535 5.7.8 Authentication credentials invalid');
    };

    const command = (line: string) => {
      state.lines.push({ text: line, secure });
      if (loginStep) {
        const step = loginStep;
        if (step === 'plain') {
          loginStep = null;
          const [, user, pass] = unb64(line).split('\0');
          finishAuth(user ?? '', pass ?? '');
        } else if (step === 'user') {
          loginUser = unb64(line);
          loginStep = 'pass';
          send(`334 ${b64('Password:')}`);
        } else {
          loginStep = null;
          finishAuth(loginUser, unb64(line));
        }
        return;
      }
      const upper = line.toUpperCase();
      if (upper.startsWith('EHLO') || upper.startsWith('HELO')) {
        const lines = capabilities();
        send(lines.map((text, i) => `250${i === lines.length - 1 ? ' ' : '-'}${text}`).join('\r\n'));
      } else if (upper === 'STARTTLS') {
        if (!context || secure || options.starttls === false || options.implicitTls) {
          send('502 5.5.1 STARTTLS is not available');
          return;
        }
        send('220 2.0.0 Ready to start TLS');
        first.removeAllListeners('data');
        const upgraded = new tls.TLSSocket(first, { isServer: true, secureContext: context });
        upgraded.on('error', () => undefined);
        socket = upgraded;
        secure = true;
        buffer = '';
        upgraded.on('data', onData);
        sockets.add(upgraded);
      } else if (upper.startsWith('AUTH')) {
        const [, mechanism, initial] = line.split(/\s+/);
        if (!mayAuthenticate()) send('538 5.7.11 Encryption required for requested authentication mechanism');
        else if (mechanism?.toUpperCase() === 'PLAIN') {
          if (initial) {
            const [, user, pass] = unb64(initial).split('\0');
            finishAuth(user ?? '', pass ?? '');
          } else {
            loginStep = 'plain';
            send('334 ');
          }
        } else if (mechanism?.toUpperCase() === 'LOGIN') {
          loginStep = 'user';
          send(`334 ${b64('Username:')}`);
        } else send('504 5.5.4 Unrecognized authentication type');
      } else if (upper.startsWith('MAIL FROM:')) {
        const reply =
          options.requireAuth && !authUser
            ? '530 5.7.0 Authentication required'
            : scripted('mail', line, '250 2.1.0 Sender ok');
        mailFrom = /<([^>]*)>/.exec(line)?.[1] ?? '';
        rcpt = [];
        mailOk = reply.startsWith('2');
        send(reply);
      } else if (upper.startsWith('RCPT TO:')) {
        const reply = mailOk
          ? scripted('rcpt', line, '250 2.1.5 Recipient ok')
          : '503 5.5.1 Error: need MAIL command';
        if (reply.startsWith('2')) rcpt.push(/<([^>]*)>/.exec(line)?.[1] ?? '');
        send(reply);
      } else if (upper === 'DATA') {
        const reply = rcpt.length
          ? scripted('data', line, '354 End data with <CR><LF>.<CR><LF>')
          : '503 5.5.1 Error: need RCPT command';
        if (reply.startsWith('3')) dataMode = true;
        send(reply);
      } else if (upper === 'RSET') send('250 2.0.0 Reset');
      else if (upper === 'NOOP') send('250 2.0.0 OK');
      else if (upper === 'QUIT') {
        closing = true;
        send('221 2.0.0 Bye');
        setTimeout(() => socket.end(), 5);
      } else send('502 5.5.2 Command not recognized');
    };

    const onData = (chunk: Buffer) => {
      buffer += chunk.toString('latin1');
      for (;;) {
        if (dataMode) {
          const end = buffer.indexOf('\r\n.\r\n');
          if (end < 0) return;
          const data = buffer.slice(0, end + 2).replace(/^\./gm, '');
          buffer = buffer.slice(end + 5);
          dataMode = false;
          const reply = scripted('end', '.', '250 2.0.0 Ok: queued');
          if (reply.startsWith('2'))
            state.messages.push({
              from: mailFrom,
              to: rcpt,
              data: Buffer.from(data, 'latin1').toString('utf8'),
              authUser,
              secure
            });
          send(reply);
          continue;
        }
        const eol = buffer.indexOf('\r\n');
        if (eol < 0) return;
        const line = buffer.slice(0, eol);
        buffer = buffer.slice(eol + 2);
        command(line);
        if (closing) return;
      }
    };

    socket.on('data', onData);
    const greet = () => send(`220 ${hostname} ESMTP ready`);
    const wait = options.greetingDelayMs ?? 0;
    if (wait === Infinity) return;
    if (wait > 0) setTimeout(greet, wait);
    else greet();
  };

  const server: net.Server =
    options.implicitTls && options.tls
      ? tls.createServer({ key: options.tls.key, cert: options.tls.cert, maxVersion: options.tls.maxVersion }, (socket) =>
          onConnection(socket, true)
        )
      : net.createServer((socket) => onConnection(socket, false));
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  state.port = (server.address() as net.AddressInfo).port;
  return state;
}
