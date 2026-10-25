// Builds one visitor sandbox on the real Dynodoc server, through its engine API: a sample
// organisation with three projects (a team handbook with its budget and review deck, a field
// guide and a grant application), their history, drafts, change requests, reviews, issues,
// labels, discussions, versions and a published copy, written by six sample colleagues
// (invented people at example.org). The visitor is a member with the roles the client's sample
// gives "you": manager, editor and contributor. Every revision's commit message is recorded in
// the service's own database (the engine has no commit messages).
import { between, seqPos, ulid } from './ulid.mjs';

export const PEOPLE = {
  alex: { email: 'alex.morgan@example.org', name: 'Alex Morgan' },
  priya: { email: 'priya.nair@example.org', name: 'Priya Nair' },
  sam: { email: 'sam.okafor@example.org', name: 'Sam Okafor' },
  lena: { email: 'lena.fischer@example.org', name: 'Lena Fischer' },
  tomas: { email: 'tomas.ruiz@example.org', name: 'Tomás Ruiz' },
  mei: { email: 'mei.chen@example.org', name: 'Mei Chen' },
};

export const para = (text, heading = false) =>
  heading
    ? { type: 'heading', attrs: { level: 2 }, content: text ? [{ type: 'text', text }] : [] }
    : { type: 'paragraph', content: text ? [{ type: 'text', text }] : [] };

/** A text document: a root and its paragraphs, keyed by short names. */
export class TextDoc {
  constructor(title) {
    this.root = ulid();
    this.nodes = new Map(); // name -> { id, pos, heading, text }
    this.title = title;
  }
  initial(list) {
    const ops = [{ type: 'NodeCreated', node_id: this.root, node_type: 'form', pos: '000000', fields: { label: this.title } }];
    list.forEach(([name, text, heading], i) => {
      const n = { id: ulid(), pos: seqPos(i * 16), heading: Boolean(heading), text };
      this.nodes.set(name, n);
      ops.push({ type: 'NodeCreated', node_id: n.id, node_type: 'item', parent_id: this.root, pos: n.pos, fields: { label: text, content: para(text, n.heading) } });
    });
    return ops;
  }
  id(name) {
    return this.nodes.get(name).id;
  }
  text(name) {
    return this.nodes.get(name).text;
  }
  edit(name, text, apply = true) {
    const n = this.nodes.get(name);
    if (apply) n.text = text;
    return [
      { type: 'FieldEdited', node_id: n.id, field: 'content', value: para(text, n.heading) },
      { type: 'FieldEdited', node_id: n.id, field: 'label', value: text },
    ];
  }
  addAfter(name, afterName, text, heading = false) {
    const sorted = [...this.nodes.values()].sort((a, b) => (a.pos < b.pos ? -1 : 1));
    const after = this.nodes.get(afterName);
    const next = sorted[sorted.indexOf(after) + 1];
    const n = { id: ulid(), pos: between(after.pos, next?.pos), heading, text };
    this.nodes.set(name, n);
    return [{ type: 'NodeCreated', node_id: n.id, node_type: 'item', parent_id: this.root, pos: n.pos, fields: { label: text, content: para(text, heading) } }];
  }
}

/** A spreadsheet: one sheet, rows keyed by name, cells by column letter. */
export class SheetDoc {
  constructor(title, sheetLabel) {
    this.root = ulid();
    this.sheet = ulid();
    this.title = title;
    this.sheetLabel = sheetLabel;
    this.rows = new Map(); // name -> { id, cells: {col: value} }
  }
  static col(letter) {
    return `cell_${letter.charCodeAt(0) - 65}`;
  }
  initial(rows) {
    const ops = [
      { type: 'NodeCreated', node_id: this.root, node_type: 'form', pos: '000000', fields: { label: this.title } },
      { type: 'NodeCreated', node_id: this.sheet, node_type: 'section', parent_id: this.root, pos: '00000000', fields: { label: this.sheetLabel, kind: 'sheet', columns: 8 } },
    ];
    rows.forEach(([name, cells], i) => {
      const id = ulid();
      this.rows.set(name, { id, cells: { ...cells } });
      const fields = {};
      for (const [c, v] of Object.entries(cells)) fields[SheetDoc.col(c)] = v;
      ops.push({ type: 'NodeCreated', node_id: id, node_type: 'item', parent_id: this.sheet, pos: seqPos(i), fields });
    });
    return ops;
  }
  set(row, col, value, apply = true) {
    const r = this.rows.get(row);
    if (apply) r.cells[col] = value;
    return [{ type: 'FieldEdited', node_id: r.id, field: SheetDoc.col(col), value }];
  }
  block(row, col) {
    return `${this.rows.get(row).id}:${SheetDoc.col(col)}`;
  }
  value(row, col) {
    return this.rows.get(row).cells[col] ?? '';
  }
}

/** A presentation: slides with a title and a body (or subtitle) shape. */
export class DeckDoc {
  constructor(title) {
    this.root = ulid();
    this.title = title;
    this.slides = new Map(); // name -> { id, pos, shapes: {role: {id, text}} }
  }
  slideOps(name, layout, title, second, pos) {
    const id = ulid();
    const shapes = {};
    const ops = [{ type: 'NodeCreated', node_id: id, node_type: 'section', parent_id: this.root, pos, fields: { label: title, kind: 'slide', layout, notes: '' } }];
    const geometry =
      layout === 'title'
        ? [
            ['title', 8.33, 18, 83.33, 32, { size: 44, align: 'center' }],
            ['subtitle', 12.5, 53, 75, 22, { size: 24, align: 'center' }],
          ]
        : [
            ['title', 6.25, 5.5, 87.5, 18, { size: 36 }],
            ['body', 6.25, 26.5, 87.5, 63, { size: 24, bullets: true }],
          ];
    geometry.forEach(([role, x, y, w, h, style], i) => {
      const text = role === 'title' ? title : (second ?? '');
      if (role !== 'title' && !text) return;
      const sid = ulid();
      shapes[role] = { id: sid, text };
      ops.push({ type: 'NodeCreated', node_id: sid, node_type: 'item', parent_id: id, pos: String(i).padStart(4, '0'), fields: { kind: 'text', role, text, x, y, w, h, style, label: text } });
    });
    this.slides.set(name, { id, pos, shapes });
    return ops;
  }
  initial(list) {
    const ops = [{ type: 'NodeCreated', node_id: this.root, node_type: 'form', pos: '000000', fields: { label: this.title, slide_size: '16:9', theme: 'office' } }];
    list.forEach(([name, layout, title, second], i) => ops.push(...this.slideOps(name, layout, title, second, seqPos(i * 16))));
    return ops;
  }
  addSlideAfter(name, afterName, title, body) {
    const sorted = [...this.slides.values()].sort((a, b) => (a.pos < b.pos ? -1 : 1));
    const after = this.slides.get(afterName);
    const next = sorted[sorted.indexOf(after) + 1];
    return this.slideOps(name, 'title-content', title, body, between(after.pos, next?.pos));
  }
  edit(slide, role, text, apply = true) {
    const s = this.slides.get(slide).shapes[role];
    if (apply) s.text = text;
    const ops = [
      { type: 'FieldEdited', node_id: s.id, field: 'text', value: text },
      { type: 'FieldEdited', node_id: s.id, field: 'label', value: text },
    ];
    if (role === 'title') ops.push({ type: 'FieldEdited', node_id: this.slides.get(slide).id, field: 'label', value: text });
    return ops;
  }
}

/**
 * Builds the sandbox. `as(person)` returns an access token for a sample person or 'visitor';
 * `note(n)` records a commit message. Returns what the clean-up needs and the visitor's
 * captured changes. `res` is filled as it goes, so a failure part-way still lists what exists.
 */
export async function buildSandbox({ api, as, visitor, note, res = { projects: [], documents: [], spaces: [] } }) {
  const P = async (who, path, body, headers) => api.post(path, body, await as(who), headers);
  const G = async (who, path) => api.get(path, await as(who));
  const ids = {};
  for (const key of Object.keys(PEOPLE)) ids[key] = (await G(key, '/auth/me')).identity_id;
  ids.visitor = visitor.identity_id;

  const space = async (who, name, kind, parent) => {
    const { id } = await P(who, '/spaces', parent ? { name, kind, parent_id: parent } : { name, kind });
    res.spaces.push({ id, owner: who });
    return id;
  };
  const member = async (who, spaceId, email, role) => P(who, `/spaces/${spaceId}/members`, { email, role });
  const settings = async (who, id, rules) => {
    const p = await G(who, `/projects/${id}`);
    await P(who, `/projects/${id}/settings`, { name: p.name, description: p.description ?? '', revision: p.revision, ...rules });
  };
  const project = async (who, name, description) => {
    const { id } = await P(who, '/projects', { name, description });
    res.projects.push({ id, owner: who });
    // The team version stays open while the sample history is written; the rules are set last.
    await settings(who, id, { protect_team_version: false, required_approvals: 0, merge_roles: 'editors' });
    return id;
  };
  const head = async (who, docId) => (await G(who, `/documents/${docId}`)).latest_seq;
  const seqNote = (docId, events, who, summary, description, kind, merged_from) => {
    const seqs = (events ?? []).map((e) => e.seq);
    if (!seqs.length) return;
    note({ document_id: docId, branch: 'main', from_seq: Math.min(...seqs), to_seq: Math.max(...seqs), kind, summary, description, merged_from, author_id: ids[who] });
  };
  const commit = async (who, docId, ops, summary, description = '') => {
    const out = await P(who, `/documents/${docId}/batch`, { ops, base_seq: await head(who, docId) });
    seqNote(docId, out.events, who, summary, description, 'revision');
  };
  const createDoc = async (who, projectId, title, kind, ops, summary) => {
    const doc = await P(who, '/documents', { title });
    res.documents.push({ id: doc.id, owner: who });
    await P(who, `/documents/${doc.id}/metadata`, { settings: { kind } });
    const out = await P(who, `/documents/${doc.id}/batch`, { ops, base_seq: 0 });
    seqNote(doc.id, out.events, who, summary, '', 'revision');
    await P(who, `/documents/${doc.id}/location`, { space_id: projectId });
    return doc.id;
  };
  const draft = async (who, docId, name, ops, summary, description = '') => {
    const { id } = await P(who, `/documents/${docId}/drafts`, { name, base_seq: await head(who, docId) });
    const out = await P(who, `/documents/${docId}/drafts/${id}`, { ops, revision: 0 });
    note({ document_id: docId, branch: id, from_seq: out.revision, to_seq: out.revision, kind: 'revision', summary, description, author_id: ids[who] });
    return id;
  };
  const submit = async (who, docId, draftId, title, message) => {
    await P(who, `/documents/${docId}/drafts/${draftId}/submit`, { note: message });
    note({ document_id: docId, branch: draftId, from_seq: 0, to_seq: 0, kind: 'request', summary: title, description: '', author_id: ids[who] });
  };
  const request = async (who, docId, draftId) => G(who, `/documents/${docId}/change-requests/${draftId}`);
  const reviewOf = async (who, docId, draftId, verdict, text) => {
    const r = await request(who, docId, draftId);
    await P(who, `/documents/${docId}/change-requests/${draftId}/reviews`, { verdict, note: text, revision: r.request.revision });
  };
  const mergeOf = async (who, docId, draftId, summary, description = '') => {
    const r = await request(who, docId, draftId);
    const out = await P(who, `/documents/${docId}/change-requests/${draftId}/merge`, { team_seq: r.team_seq, revision: r.revision });
    seqNote(docId, out.events, who, summary, description, 'merge', draftId);
  };
  const version = async (who, docId, name, text = '') => P(who, `/documents/${docId}/versions`, { name, note: text });

  // ---- people and spaces ----------------------------------------------------------------------
  const org = await space('alex', 'Sample organisation', 'organization');
  await member('alex', org, visitor.email, 'manager');
  const team = await space('alex', 'Operations team', 'team', org);
  for (const [who, role] of [['priya', 'editor'], ['sam', 'editor'], ['lena', 'approver'], ['tomas', 'editor'], ['mei', 'reviewer']]) await member('alex', team, PEOPLE[who].email, role);

  // ---- Team handbook ----------------------------------------------------------------------------
  const p1 = await project('alex', 'Team handbook', "The team's handbook, its yearly budget and the quarterly review deck.");
  await member('alex', p1, visitor.email, 'manager');
  for (const [who, role] of [['priya', 'editor'], ['sam', 'editor'], ['tomas', 'editor'], ['lena', 'approver'], ['mei', 'editor']]) await member('alex', p1, PEOPLE[who].email, role);

  const hb = new TextDoc('Team handbook');
  const hbId = await createDoc('alex', p1, 'Team handbook', 'document', hb.initial([
    ['p1', 'Welcome to the team', true],
    ['p2', 'This handbook describes how we work together: our hours, how we plan, how we share documents and how we look after each other.'],
    ['p3', 'Working hours', true],
    ['p4', 'Core hours are 10:00 to 16:00.'],
    ['p5', 'Remote work', true],
    ['p6', 'You may work remotely up to two days a week. Agree the days with your manager at the start of each month.'],
    ['p8', 'Leave', true],
    ['p9', 'Everyone has 22 days of annual leave plus public holidays.'],
    ['p10', 'Parental leave is 12 weeks at full pay.'],
    ['p11', 'Expenses', true],
    ['p12', 'Submit expenses within 60 days with a receipt. Travel is booked through the shared travel inbox.'],
  ]), 'Create the team handbook');
  await version('alex', hbId, 'First edition');
  await commit('priya', hbId, hb.edit('p4', 'Core hours are 10:00 to 15:00 in your local time zone. Outside core hours, plan your day as suits you and your team.'), 'Shorten core hours', 'Agreed in the September planning meeting.');
  await commit('alex', hbId, hb.edit('p6', 'You may work remotely up to three days a week. Agree the days with your manager at the start of each month.'), 'Allow three remote days a week');
  await commit('sam', hbId, hb.addAfter('p7', 'p6', 'The team meets in person on the first Tuesday of every month.'), 'Add the monthly in-person day');
  await commit('alex', hbId, hb.edit('p9', 'Everyone has 25 days of annual leave plus public holidays.'), 'Annual leave goes to 25 days');
  await settings('alex', p1, { protect_team_version: false, required_approvals: 2, merge_roles: 'editors' });

  const expenses = 'Submit expenses within 30 days with a receipt. Travel is booked through the shared travel inbox.';
  const d9 = await draft('priya', hbId, 'expenses-deadline', hb.edit('p12', expenses, false), 'Expenses within 30 days');
  await submit('priya', hbId, d9, 'Expenses within 30 days', 'Matches the finance page.');
  await reviewOf('alex', hbId, d9, 'approved', '');
  await reviewOf('lena', hbId, d9, 'approved', 'Thanks.');
  await mergeOf('alex', hbId, d9, 'Merge: Expenses within 30 days', 'Matches the finance page. Closes the expenses issue.');
  hb.nodes.get('p12').text = expenses;

  await commit('alex', hbId, [...hb.addAfter('p13', 'p12', 'Documents And Review', true), ...hb.addAfter('p14', 'p13', 'Shared documents live in Dynodoc. Edits to the handbook go through a change request and need two approvals.')], 'Add the documents and review section');
  await version('alex', hbId, 'Autumn 2026 edition', 'Shared with new starters from October.');

  const d10 = await draft('sam', hbId, 'four-day-week', hb.edit('p4', 'From January the team trials a four-day week: core hours are 09:00 to 16:00, Monday to Thursday.', false), 'Trial a four-day week');
  await submit('sam', hbId, d10, 'Trial a four-day week', 'For discussion.');
  await P('alex', `/documents/${hbId}/change-requests/${d10}/decline`, { note: 'Not this year; let us revisit in the spring.' });

  const d14 = await draft('priya', hbId, 'parental-leave', hb.edit('p10', 'Parental leave is 16 weeks at full pay for every parent, whoever gave birth, and can be taken in up to three blocks.', false), 'Parental leave: 16 weeks for every parent', 'Brings us in line with the policy the board approved.');
  await submit('priya', hbId, d14, 'Parental leave: 16 weeks for every parent', 'Brings the handbook in line with the policy the board approved on 2 October.');
  await reviewOf('lena', hbId, d14, 'commented', 'Can we say who to tell, and how early?');
  await reviewOf('alex', hbId, d14, 'approved', 'Looks right to me.');

  await commit('tomas', hbId, hb.edit('p9', 'Everyone has 25 days of annual leave plus public holidays. Request leave at least two weeks ahead in the leave calendar.'), 'Say how to request leave');
  await commit('priya', hbId, hb.edit('p13', 'Documents and review'), 'Fix heading case');

  // The visitor's own private draft.
  const mine = await draft('visitor', hbId, 'remote-work-update', hb.edit('p6', 'You may work remotely up to three days a week, and from another country for up to four weeks a year. Agree the days with your manager at the start of each month.', false), 'Allow working from another country', 'Answers the remote work issue.');

  // Discussions and a published copy.
  const t1 = await P('tomas', `/documents/${hbId}/ops/comment`, { node_id: hb.id('p7'), body: 'Does the first Tuesday still work for the people in other time zones?' });
  const t2 = await P('mei', `/documents/${hbId}/ops/comment`, { node_id: hb.id('p4'), body: 'Core hours read well now.' });
  const threads = (await G('alex', `/documents/${hbId}/discussions`)).items ?? [];
  const th1 = threads.find((t) => t.id === t1.event?.id);
  if (th1) await P('alex', `/documents/${hbId}/discussions/${th1.id}`, { revision: th1.revision, body: 'We will check in the next planning meeting.' });
  const th2 = threads.find((t) => t.id === t2.event?.id);
  if (th2) await P('alex', `/documents/${hbId}/discussions/${th2.id}`, { revision: th2.revision, resolved: true });
  await P('alex', `/documents/${hbId}/public-links`, { base_seq: await head('alex', hbId), expires_days: 30 });

  // ---- Budget 2027 --------------------------------------------------------------------------------
  const bu = new SheetDoc('Budget 2027', 'Budget');
  const buId = await createDoc('sam', p1, 'Budget 2027', 'spreadsheet', bu.initial([
    ['head', { A: 'Line', C: 'Plan', D: 'Forecast' }],
    ['sal', { A: 'Salaries', C: '412000', D: '409500' }],
    ['con', { A: 'Contractors', C: '48000', D: '48000' }],
    ['sw', { A: 'Software', C: '16800', D: '18600' }],
    ['trv', { A: 'Travel', C: '24000', D: '24000' }],
    ['trn', { A: 'Training', C: '9000', D: '9000' }],
    ['off', { A: 'Office', C: '30000', D: '30000' }],
    ['tot', { A: 'Total', C: '=SUM(C2:C7)', D: '=SUM(D2:D7)' }],
  ]), 'Create the 2027 budget');
  await commit('sam', buId, bu.set('con', 'D', '52500'), 'Contractors forecast up after Q2');
  const d11 = await draft('mei', buId, 'software-renewals', bu.set('sw', 'C', '18600', false), 'Plan software renewals at the new price');
  await submit('mei', buId, d11, 'Plan software renewals at the new price', '');
  await reviewOf('sam', buId, d11, 'approved', '');
  await reviewOf('alex', buId, d11, 'approved', '');
  await mergeOf('alex', buId, d11, 'Merge: Plan software renewals at the new price');
  bu.rows.get('sw').cells.C = '18600';
  await version('alex', buId, 'Approved plan', 'As approved by the board.');
  await commit('sam', buId, bu.set('off', 'D', '31200'), 'Office forecast after the lease renewal');
  const d15 = await draft('sam', buId, 'travel-reforecast', bu.set('trv', 'D', '26800', false), 'Travel forecast for the January offsite', 'Adds the offsite: 14 people, two nights.');
  await commit('mei', buId, bu.set('trv', 'D', '21400'), 'Travel forecast down: fewer conference trips');
  await submit('sam', buId, d15, 'Travel forecast for the January offsite', 'Adds the offsite: 14 people, two nights.');

  // ---- Quarterly review ---------------------------------------------------------------------------
  const dk = new DeckDoc('Quarterly review');
  const dkId = await createDoc('mei', p1, 'Quarterly review', 'presentation', dk.initial([
    ['s1', 'title', 'Quarterly review: Q3 2026', 'Team update, September 2026'],
    ['s2', 'title-content', 'Highlights', 'Handbook moved to Dynodoc · Budget forecast close to plan · Two new team members'],
    ['s3', 'title-content', 'Budget', 'Forecast 539,100 against a plan of 539,800.'],
    ['s5', 'title-content', 'Questions', ''],
  ]), 'Create the Q3 review deck');
  await commit('mei', dkId, dk.edit('s3', 'body', 'Forecast 544,800 against a plan of 541,600. Contractors and office above plan; travel on plan.'), 'Update the budget slide');
  await commit('alex', dkId, dk.addSlideAfter('s4', 's3', 'Next quarter', 'Finish the remote work policy · Review the training budget · Plan the January offsite'), 'Add the next quarter slide');
  const d16 = await draft('mei', dkId, 'fix-title-date', dk.edit('s1', 'subtitle', 'Team update, October 2026', false), 'Fix the month on the title slide');
  await submit('mei', dkId, d16, 'Fix the month on the title slide', 'The deck covers the quarter that ended on 30 September and is presented in October.');

  // ---- labels, issues, stars ------------------------------------------------------------------------
  const label = async (name, color, description) => (await P('alex', `/projects/${p1}/labels`, { name, color, description })).id;
  const lPolicy = await label('policy', '6d28d9', 'Rules in the handbook');
  const lBudget = await label('budget', '0f766e', 'Budget lines');
  const lQuestion = await label('question', 'b45309', 'Needs an answer');
  const closed = await P('priya', `/projects/${p1}/issues`, { title: 'Expenses deadline differs between handbook and finance page', body: 'Handbook said 60 days, finance said 30.', labels: [lPolicy] });
  await P('priya', `/projects/${p1}/issues/${closed.number}/comments`, { body: 'Fixed by the expenses change request.' });
  await api.patch(`/projects/${p1}/issues/${closed.number}`, { state: 'closed' }, await as('priya'));
  const remote = await P('tomas', `/projects/${p1}/issues`, { title: 'Remote work section does not mention working from another country', body: 'Several people asked whether they can work from abroad for a few weeks. The handbook is silent on it.', labels: [lPolicy], assignee_id: ids.visitor });
  await P('alex', `/projects/${p1}/issues/${remote.number}/comments`, { body: 'Agreed. Up to four weeks a year seems to be the common answer.' });
  await P('lena', `/projects/${p1}/issues`, { title: 'Training budget looks low for next year', body: 'Two courses are already booked for Q1. The plan line is 9,000.', labels: [lBudget, lQuestion] });
  for (const who of ['alex', 'priya', 'sam', 'lena']) await api.put(`/projects/${p1}/star`, undefined, await as(who));

  // ---- Field guide ----------------------------------------------------------------------------------
  const p2 = await project('alex', 'Field guide', 'How the team runs site visits: safety, checklists and contacts.');
  await member('alex', p2, visitor.email, 'editor');
  await member('alex', p2, PEOPLE.tomas.email, 'editor');
  await member('alex', p2, PEOPLE.lena.email, 'approver');
  const fg = new TextDoc('Field guide');
  const fgId = await createDoc('alex', p2, 'Field guide', 'document', fg.initial([
    ['f1', 'Before a site visit', true],
    ['f2', 'Tell the site contact the day before. Check the weather and the access notes.'],
    ['f3', 'On site', true],
    ['f4', 'Wear the safety kit. Visits are done in pairs.'],
    ['f5', 'After the visit', true],
    ['f6', 'Upload notes and photos within two working days.'],
  ]), 'Create the field guide');
  await commit('tomas', fgId, fg.edit('f4', 'Wear the safety kit. Visits are done in pairs, with a check-in call to the office every three hours.'), 'Add the check-in call');
  await api.put(`/projects/${p2}/star`, undefined, await as('tomas'));
  await settings('alex', p2, { protect_team_version: true, required_approvals: 1, merge_roles: 'owners' });

  // ---- Grant application ----------------------------------------------------------------------------
  const p3 = await project('lena', 'Grant application', 'Narrative and budget for the community skills grant.');
  await member('lena', p3, visitor.email, 'reviewer');
  await member('lena', p3, PEOPLE.priya.email, 'editor');
  const gn = new TextDoc('Grant narrative');
  await createDoc('lena', p3, 'Grant narrative', 'document', gn.initial([
    ['g1', 'Community skills programme', true],
    ['g2', 'We will run twelve evening workshops on spreadsheets and budgeting for local volunteers.'],
    ['g3', 'Each workshop takes up to 20 people. Materials are published under an open licence.'],
  ]), 'First draft of the narrative');
  const gb = new SheetDoc('Grant budget', 'Lines');
  const gbId = await createDoc('lena', p3, 'Grant budget', 'spreadsheet', gb.initial([
    ['head', { A: 'Line', B: 'Amount' }],
    ['tr', { A: 'Trainers', B: '7200' }],
    ['ve', { A: 'Venue', B: '2400' }],
    ['ma', { A: 'Materials', B: '900' }],
    ['to', { A: 'Total', B: '=SUM(B2:B4)' }],
  ]), 'Grant budget lines');
  await commit('priya', gbId, gb.set('ve', 'B', '1800'), 'Venue at the community centre rate');
  await settings('lena', p3, { protect_team_version: true, required_approvals: 1, merge_roles: 'owners' });

  // ---- what the visitor's editor add-ins captured and nobody committed yet --------------------------
  const now = Date.now();
  const MIN = 60_000;
  const w = (id, docId, branch, staged, source, ago, edit) => ({ id, docId, branch, staged, source, capturedAt: now - ago * MIN, edit });
  const s4 = dk.slides.get('s4');
  const working = [
    w('w1', hbId, 'main', true, 'word', 40, { blockId: hb.id('p12'), op: 'edit', before: hb.text('p12'), after: 'Submit expenses within 30 days with a receipt. Travel is booked through the shared travel inbox at least ten days ahead.' }),
    w('w2', hbId, 'main', false, 'word', 35, { blockId: hb.id('p2'), op: 'edit', before: hb.text('p2'), after: 'This handbook describes how we work together: our hours, how we plan, how we share documents, and how we look after one another.' }),
    w('w3', buId, 'main', true, 'excel', 22, { blockId: bu.block('trn', 'C'), op: 'edit', before: bu.value('trn', 'C'), after: '12000' }),
    w('w5', dkId, 'main', false, 'powerpoint', 9, { blockId: s4.shapes.body.id, op: 'edit', before: s4.shapes.body.text, after: 'Finish the remote work policy · Review the training budget (+3,000 in the plan) · Plan the January offsite' }),
    w('w6', dkId, 'main', false, 'powerpoint', 8, { blockId: `${s4.id}:notes`, op: 'add', afterId: s4.shapes.body.id, label: 'Slide 4 · Notes', after: 'Ask Sam for the offsite numbers before the meeting.' }),
    w('w7', hbId, mine, true, 'word', 15, { blockId: hb.id('p7'), op: 'edit', before: hb.text('p7'), after: 'The team meets in person on the first Tuesday of every month. Travel to it is paid from the team budget.' }),
  ];
  return { resources: res, working, projects: [p1, p2, p3], firstProject: p1 };
}
