// Retry policy for one outgoing message: a few attempts, exponential backoff with jitter, only
// for failures that can clear by themselves (a 4xx reply, a refused or lost connection, a
// timeout), never for a 5xx reply, a refused certificate or a refused message. The whole send,
// retries included, stays inside one time budget. Time and randomness are parameters so tests
// run on a fake clock.
import { classifyError } from './errors.js';

export const DEFAULT_RETRY = Object.freeze({
  maxAttempts: 3,
  baseDelayMs: 1000,
  maxDelayMs: 8000,
  /** An attempt needs at least this long to be worth starting. */
  minAttemptMs: 3000
});

/**
 * Delay after the n-th failed attempt (1-based): exponential, capped, then jittered into the
 * upper half so simultaneous senders do not retry in step.
 * @param {number} failed
 * @param {{ baseDelayMs: number, maxDelayMs: number }} policy
 * @param {() => number} random
 */
export function backoffDelay(failed, policy, random) {
  const ceiling = Math.min(policy.maxDelayMs, policy.baseDelayMs * 2 ** (failed - 1));
  return Math.round(ceiling * (0.5 + random() * 0.5));
}

/**
 * Run `attempt` until it succeeds, fails for good or runs out of attempts or time.
 * @template T
 * @param {(info: { attempt: number, remainingMs: number }) => Promise<T>} attempt
 * @param {{
 *   budgetMs: number,
 *   policy?: typeof DEFAULT_RETRY,
 *   now?: () => number,
 *   sleep?: (ms: number) => Promise<void>,
 *   random?: () => number,
 *   onRetry?: (info: { attempt: number, errorClass: string, delayMs: number }) => void
 * }} options
 * @returns {Promise<{ value: T, attempts: number }>}
 */
export async function withRetry(attempt, options) {
  const policy = options.policy ?? DEFAULT_RETRY;
  const now = options.now ?? Date.now;
  const sleep = options.sleep ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
  const random = options.random ?? Math.random;
  const deadline = now() + options.budgetMs;
  for (let number = 1; ; number++) {
    try {
      const value = await attempt({ attempt: number, remainingMs: Math.max(0, deadline - now()) });
      return { value, attempts: number };
    } catch (raw) {
      const error = classifyError(raw);
      const delayMs = backoffDelay(number, policy, random);
      const roomLeft = deadline - now() - delayMs >= policy.minAttemptMs;
      if (!error.retryable || number >= policy.maxAttempts || !roomLeft) {
        /** @type {any} */ (error).attempts = number;
        throw error;
      }
      options.onRetry?.({ attempt: number, errorClass: error.errorClass, delayMs });
      await sleep(delayMs);
    }
  }
}
