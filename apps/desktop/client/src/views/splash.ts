// The start-up screen: shown while the app starts the workspace on this computer (the first launch
// creates the database, which takes a little while), asks a new person for a name and a first
// project, and says plainly what went wrong when the workspace did not start.
import { h } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render } from '../state.ts';
import { button, field, input, notice } from '../ui/controls.ts';
import { createProfile, openLocalFolder, retryLocal } from '../data/local.ts';
import { resetSample } from '../state.ts';

function card(...children: (HTMLElement | null)[]): HTMLElement {
  return h(
    'div',
    { class: 'splash' },
    h('div', { class: 'splash-card card' }, h('div', { class: 'splash-brand' }, h('img', { src: `${__ASSET_BASE__}mark.svg`, alt: '', width: 22, height: 22 }), h('span', null, 'Dynodoc')), ...children),
  );
}

function starting(): HTMLElement {
  const l = state.local;
  return card(
    h('div', { class: 'splash-step', role: 'status' }, icon('sync', 16, 'icon spin'), h('span', null, l.step || 'Starting')),
    h('p', { class: 'muted small' }, l.firstRun ? 'The first start makes your local database and can take a minute or two. It happens once.' : 'Your files are on this computer; nothing is sent anywhere.'),
  );
}

function profileForm(): HTMLElement {
  const f = state.local.form;
  return card(
    h('h1', { class: 'splash-title' }, 'Welcome to Dynodoc'),
    h('p', { class: 'muted' }, 'Everything here stays on this computer: your documents, their history and your reviews live in a database the app keeps for you. You do not need an account. You can sign in to a server later to share and sync.'),
    h(
      'form',
      {
        onSubmit: (e: Event) => {
          e.preventDefault();
          void createProfile();
        },
      },
      field('Your name', input(f.name, (v) => { f.name = v; }, { id: 'profile-name', placeholder: 'Priya Raman', maxlength: 80, autofocus: true, disabled: f.busy }), 'Shown on the changes you make and the reviews you write.'),
      field('Email address (optional)', input(f.email, (v) => { f.email = v; }, { id: 'profile-email', placeholder: 'priya@example.org', maxlength: 254, disabled: f.busy }), 'Only a label in your local database. Leave it empty to use a private placeholder.'),
      field('First project', input(f.project, (v) => { f.project = v; }, { id: 'profile-project', maxlength: 100, disabled: f.busy }), 'A project holds files, their history and the people who review them. You can add more.'),
      f.error ? notice('error', f.error) : null,
      h('div', { class: 'splash-actions' }, button(f.busy ? 'Creating…' : 'Create my workspace', { variant: 'primary', type: 'submit', id: 'profile-go', disabled: f.busy })),
    ),
  );
}

function failed(): HTMLElement {
  const l = state.local;
  return card(
    h('h1', { class: 'splash-title' }, 'The workspace on this computer did not start'),
    notice('error', l.detail || 'Something went wrong while starting.'),
    h('p', { class: 'muted small' }, 'Your data is not touched by a failed start. The logs are in the workspace folder.'),
    h(
      'div',
      { class: 'splash-actions' },
      button('Try again', { variant: 'primary', id: 'local-retry', onClick: () => void retryLocal() }),
      button('Open the logs folder', { onClick: () => void openLocalFolder() }),
      button('Use the offline sample', { onClick: () => { resetSample(); state.local.splash = false; state.live.mode = 'offline'; state.live.env = 'sample'; render(); } }),
    ),
  );
}

export function splash(): HTMLElement | null {
  if (!state.local.splash) return null;
  if (state.local.phase === 'failed') return failed();
  if (state.local.phase === 'profile') return profileForm();
  return starting();
}
