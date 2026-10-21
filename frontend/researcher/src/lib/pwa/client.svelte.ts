/**
 * The page's side of the installable app: registers the service worker, tracks a waiting
 * update, and holds the browser's install prompt. An update is only ever applied when the
 * person asks for it; nothing here reloads a page by itself.
 */
import { installGuide, type InstallGuide } from './install';
import { isPrivateArea } from './sw-policy';

/** Chromium's install prompt event; not in the standard DOM typings. */
interface InstallPromptEvent extends Event {
  prompt(): Promise<void>;
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

const HOUR = 3_600_000;

function isStandalone() {
  return (
    window.matchMedia?.('(display-mode: standalone)').matches ||
    window.matchMedia?.('(display-mode: minimal-ui)').matches ||
    (navigator as Navigator & { standalone?: boolean }).standalone === true
  );
}

class PwaState {
  /** A new version has installed and is waiting for the person to accept it. */
  updateReady = $state(false);
  /** The person chose "Not now"; the notice stays hidden until the next visit. */
  updateDeferred = $state(false);
  installed = $state(false);
  canPrompt = $state(false);
  guide = $state<InstallGuide | null>(null);
  #registration: ServiceWorkerRegistration | null = null;
  #prompt: InstallPromptEvent | null = null;
  #applying = false;
  #started = false;

  /** Call once in the browser, from the root layout. */
  start() {
    if (this.#started || typeof window === 'undefined') return;
    this.#started = true;
    this.installed = isStandalone();
    window.addEventListener('beforeinstallprompt', (event) => {
      // Keep the prompt for the profile page instead of showing the browser's own bar.
      event.preventDefault();
      this.#prompt = event as InstallPromptEvent;
      this.canPrompt = true;
      this.#refreshGuide();
    });
    window.addEventListener('appinstalled', () => {
      this.#prompt = null;
      this.canPrompt = false;
      this.installed = true;
      this.#refreshGuide();
    });
    this.#refreshGuide();
    // Production builds only: the development server has no built files to cache.
    if (import.meta.env.DEV || !('serviceWorker' in navigator)) return;
    // Sign-in, shared links, admin and the Office panes never get a worker from this page.
    if (isPrivateArea(location.pathname)) return;
    const register = () => void this.#register();
    if (document.readyState === 'complete') register();
    else window.addEventListener('load', register, { once: true });
  }

  #refreshGuide() {
    this.guide = installGuide({
      userAgent: navigator.userAgent,
      platform: navigator.platform,
      maxTouchPoints: navigator.maxTouchPoints,
      standalone: this.installed,
      canPrompt: this.canPrompt,
      secure: window.isSecureContext
    });
  }

  async #register() {
    try {
      const registration = await navigator.serviceWorker.register('/service-worker.js', {
        scope: '/',
        updateViaCache: 'none'
      });
      this.#registration = registration;
      const watch = (worker: ServiceWorker | null) => {
        if (!worker) return;
        const settled = () => {
          // The first worker ever installed has no controller to replace; only a worker
          // that waits behind a running one is an update.
          if (worker.state === 'installed' && navigator.serviceWorker.controller)
            this.updateReady = true;
        };
        worker.addEventListener('statechange', settled);
        settled();
      };
      watch(registration.installing);
      watch(registration.waiting);
      registration.addEventListener('updatefound', () => watch(registration.installing));
      // Single-page navigation never reloads, so look for a new version when the person
      // comes back to the tab, at most once an hour.
      let checked = Date.now();
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState !== 'visible' || Date.now() - checked < HOUR) return;
        checked = Date.now();
        void registration.update().catch(() => {});
      });
    } catch {
      /* No worker: the app works as a normal website. */
    }
  }

  /** Ask the browser to show its install dialog. Resolves true if the person accepted. */
  async install(): Promise<boolean> {
    const prompt = this.#prompt;
    if (!prompt) return false;
    // A prompt can be used once.
    this.#prompt = null;
    this.canPrompt = false;
    try {
      await prompt.prompt();
      const { outcome } = await prompt.userChoice;
      if (outcome === 'accepted') this.installed = true;
      return outcome === 'accepted';
    } finally {
      this.#refreshGuide();
    }
  }

  /**
   * Switch to the waiting version and reload once it has taken over. Only called from a
   * button. The reload goes through the normal unload path, so an editor holding unsaved
   * work still gets the browser's leave-page warning.
   */
  applyUpdate() {
    const waiting = this.#registration?.waiting;
    if (!waiting) return;
    this.#applying = true;
    this.updateReady = false;
    navigator.serviceWorker.addEventListener(
      'controllerchange',
      () => {
        if (this.#applying) location.reload();
      },
      { once: true }
    );
    waiting.postMessage({ type: 'SKIP_WAITING' });
  }

  deferUpdate() {
    this.updateDeferred = true;
  }
}

export const pwa = new PwaState();
