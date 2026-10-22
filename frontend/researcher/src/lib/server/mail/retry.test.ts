// The retry policy on a fake clock: nothing here waits for real time.
import { describe, expect, it } from 'vitest';
import { DEFAULT_RETRY, backoffDelay, withRetry } from './retry.js';
import { MailError } from './errors.js';

function clock(start = 0) {
  let time = start;
  const sleeps: number[] = [];
  return {
    now: () => time,
    sleep: async (ms: number) => {
      sleeps.push(ms);
      time += ms;
    },
    advance: (ms: number) => (time += ms),
    sleeps
  };
}
const smtpError = (responseCode: number, command = 'RCPT TO', code = 'EENVELOPE') =>
  Object.assign(new Error('server said no'), {
    responseCode,
    command,
    code,
    response: `${responseCode} reply`
  });

describe('backoff', () => {
  it('doubles from the base, is capped, and is jittered into the upper half', () => {
    const policy = { baseDelayMs: 1000, maxDelayMs: 8000 };
    expect(backoffDelay(1, policy, () => 1)).toBe(1000);
    expect(backoffDelay(2, policy, () => 1)).toBe(2000);
    expect(backoffDelay(3, policy, () => 1)).toBe(4000);
    expect(backoffDelay(9, policy, () => 1)).toBe(8000);
    expect(backoffDelay(3, policy, () => 0)).toBe(2000);
    expect(backoffDelay(3, policy, () => 0.5)).toBe(3000);
  });
});

describe('withRetry', () => {
  it('returns at once on success', async () => {
    const time = clock();
    const result = await withRetry(async () => 'sent', { budgetMs: 30_000, ...time });
    expect(result).toEqual({ value: 'sent', attempts: 1 });
    expect(time.sleeps).toEqual([]);
  });
  it('retries a 4xx reply with backoff, then succeeds', async () => {
    const time = clock();
    let calls = 0;
    const result = await withRetry(
      async () => {
        if (++calls < 3) throw smtpError(451);
        return 'sent';
      },
      { budgetMs: 60_000, random: () => 1, ...time }
    );
    expect(result).toEqual({ value: 'sent', attempts: 3 });
    expect(time.sleeps).toEqual([1000, 2000]);
  });
  it('retries refused connections, timeouts and lost connections', async () => {
    for (const error of [
      Object.assign(new Error('connect ECONNREFUSED 127.0.0.1:25'), { code: 'ESOCKET' }),
      Object.assign(new Error('Timeout'), { code: 'ETIMEDOUT', command: 'CONN' }),
      Object.assign(new Error('read ECONNRESET'), { code: 'ECONNRESET' }),
      smtpError(421, 'EHLO', 'ECONNECTION')
    ]) {
      const time = clock();
      let calls = 0;
      const result = await withRetry(
        async () => {
          if (++calls === 1) throw error;
          return 'sent';
        },
        { budgetMs: 60_000, ...time }
      );
      expect(result.attempts, String(error.message)).toBe(2);
    }
  });
  it('never retries a 5xx reply', async () => {
    for (const status of [500, 501, 550, 552, 553, 554]) {
      const time = clock();
      let calls = 0;
      await expect(
        withRetry(
          async () => {
            calls++;
            throw smtpError(status);
          },
          { budgetMs: 60_000, ...time }
        )
      ).rejects.toBeInstanceOf(MailError);
      expect(calls, String(status)).toBe(1);
      expect(time.sleeps).toEqual([]);
    }
  });
  it('never retries what a retry cannot fix: authentication, certificates, refused messages', async () => {
    const cases: Error[] = [
      Object.assign(new Error('Invalid login'), { code: 'EAUTH', responseCode: 535 }),
      Object.assign(new Error('self-signed certificate'), { code: 'ESOCKET' }),
      Object.assign(new Error("Hostname/IP does not match certificate's altnames"), {
        code: 'ESOCKET'
      }),
      Object.assign(new Error('Error upgrading connection with STARTTLS'), {
        code: 'ETLS',
        command: 'STARTTLS',
        responseCode: 502
      }),
      Object.assign(new Error('getaddrinfo ENOTFOUND mail.example.invalid'), { code: 'EDNS' }),
      new MailError('invalid-message'),
      new MailError('too-large')
    ];
    for (const error of cases) {
      let calls = 0;
      await expect(
        withRetry(
          async () => {
            calls++;
            throw error;
          },
          { budgetMs: 60_000, ...clock() }
        )
      ).rejects.toBeInstanceOf(MailError);
      expect(calls, error.message).toBe(1);
    }
  });
  it('gives up after the attempt limit and says how many were made', async () => {
    const time = clock();
    let calls = 0;
    const failure = await withRetry(
      async () => {
        calls++;
        throw smtpError(450);
      },
      { budgetMs: 600_000, ...time }
    ).catch((error) => error);
    expect(calls).toBe(DEFAULT_RETRY.maxAttempts);
    expect(failure).toBeInstanceOf(MailError);
    expect(failure.errorClass).toBe('transient');
    expect(failure.attempts).toBe(DEFAULT_RETRY.maxAttempts);
  });
  it('stays inside the time budget: no sleep that leaves too little time for another attempt', async () => {
    const time = clock();
    let calls = 0;
    const failure = await withRetry(
      async () => {
        calls++;
        time.advance(7000); // a slow attempt
        throw smtpError(451);
      },
      { budgetMs: 10_000, random: () => 1, ...time }
    ).catch((error) => error);
    expect(calls).toBe(1);
    expect(time.sleeps).toEqual([]);
    expect(failure.errorClass).toBe('transient');
  });
  it('tells each attempt how much of the budget is left', async () => {
    const time = clock();
    const left: number[] = [];
    let calls = 0;
    await withRetry(
      async ({ remainingMs }) => {
        left.push(remainingMs);
        if (++calls < 3) throw smtpError(451);
      },
      { budgetMs: 30_000, random: () => 1, ...time }
    );
    expect(left).toEqual([30_000, 29_000, 27_000]);
  });
  it('reports each retry so it can be logged by class, not by message', async () => {
    const seen: unknown[] = [];
    let calls = 0;
    await withRetry(
      async () => {
        if (++calls === 1) throw smtpError(452);
      },
      { budgetMs: 30_000, random: () => 1, ...clock(), onRetry: (info) => seen.push(info) }
    );
    expect(seen).toEqual([{ attempt: 1, errorClass: 'transient', delayMs: 1000 }]);
  });
});
