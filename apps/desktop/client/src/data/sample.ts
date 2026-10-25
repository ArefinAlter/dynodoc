// The neutral sample the client opens on when it is not signed in to a server: a team's
// handbook, its yearly budget and its quarterly review deck, with a plausible history of edits,
// drafts and change requests. Every person, address and figure is invented (example.org).
import type { Block, Edit } from '../lib/diff.ts';
import { addCommit, conflictsOf, draftEdits, mainSeq } from './backend.ts';
import type { AddinHost, Db, Doc, Draft, FileKind, Project, Role } from './types.ts';

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

type Change = ['edit', string, string] | ['add', string, string, string, string] | ['remove', string];

function edits(blocks: Block[], changes: Change[]): Edit[] {
  return changes.map((c): Edit => {
    if (c[0] === 'edit') return { blockId: c[1], op: 'edit', before: blocks.find((b) => b.id === c[1])?.text ?? '', after: c[2] };
    if (c[0] === 'add') return { blockId: c[1], op: 'add', label: c[2], afterId: c[3], after: c[4] };
    const i = blocks.findIndex((b) => b.id === c[1]);
    return { blockId: c[1], op: 'remove', before: blocks[i]?.text ?? '', label: blocks[i]?.label, afterId: i > 0 ? blocks[i - 1].id : '' };
  });
}

export function seed(now = Date.now()): Db {
  const db: Db = {
    me: 'u_jordan',
    people: [
      { id: 'u_jordan', name: 'Jordan Lee', email: 'jordan.lee@example.org' },
      { id: 'u_alex', name: 'Alex Morgan', email: 'alex.morgan@example.org' },
      { id: 'u_priya', name: 'Priya Nair', email: 'priya.nair@example.org' },
      { id: 'u_sam', name: 'Sam Okafor', email: 'sam.okafor@example.org' },
      { id: 'u_lena', name: 'Lena Fischer', email: 'lena.fischer@example.org' },
      { id: 'u_tomas', name: 'Tomás Ruiz', email: 'tomas.ruiz@example.org' },
      { id: 'u_mei', name: 'Mei Chen', email: 'mei.chen@example.org' },
      { id: 'u_agent', name: 'Review assistant', email: 'agent key of Alex Morgan', agent: true },
    ],
    spaces: [
      {
        id: 'sp_org',
        name: 'Sample organisation',
        kind: 'organization',
        members: [
          { personId: 'u_alex', role: 'owner', via: 'direct' },
          { personId: 'u_jordan', role: 'manager', via: 'direct' },
        ],
      },
      {
        id: 'sp_ops',
        name: 'Operations team',
        kind: 'team',
        parentId: 'sp_org',
        members: [
          { personId: 'u_priya', role: 'editor', via: 'direct' },
          { personId: 'u_sam', role: 'editor', via: 'direct' },
          { personId: 'u_lena', role: 'approver', via: 'direct' },
          { personId: 'u_tomas', role: 'editor', via: 'direct' },
          { personId: 'u_mei', role: 'reviewer', via: 'direct' },
        ],
      },
    ],
    projects: [],
    docs: [],
    commits: [],
    drafts: [],
    requests: [],
    working: [],
    notifications: [],
    activity: [],
    deviceKeys: [
      { id: 'key_laptop', name: "Jordan's laptop", kind: 'person', publicKey: 'b3f1c27e9a40d85e6c1f07b2a9d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f', createdAt: now - 40 * DAY, revoked: false, scopes: ['project:read', 'history:read', 'history:write', 'history:review'] },
      { id: 'key_agent', name: 'Review assistant', kind: 'agent', publicKey: '5e8a1d2c3b4f6a7e8d9c0b1a2f3e4d5c6b7a8f9e0d1c2b3a4f5e6d7c8b9a0f1e', createdAt: now - 12 * DAY, revoked: false, scopes: ['content:read'] },
    ],
    lastFetched: now - 4 * MIN,
    incoming: 0,
  };

  const members = (roles: [string, Role][]) => roles.map(([personId, role]) => ({ personId, role, via: 'direct' as const }));
  const policy = { enabled: true, minRole: 'editor' as Role, callsPerMinute: 30, openProposals: 3, proposalsPerDay: 10, submitRequiresPerson: true };

  const proj = (p: Partial<Project> & Pick<Project, 'id' | 'name' | 'description' | 'role' | 'rules'>): Project => {
    const full: Project = {
      ownerId: 'u_alex',
      private: true,
      starred: false,
      stars: 0,
      watch: 'requests',
      updatedAt: now,
      members: [],
      issues: [],
      labels: [],
      agentPolicy: { ...policy },
      space: 'sp_ops',
      ...p,
    };
    db.projects.push(full);
    return full;
  };

  const handbookProject = proj({
    id: 'p_handbook',
    name: 'Team handbook',
    description: "The team's handbook, its yearly budget and the quarterly review deck.",
    role: 'manager',
    starred: true,
    stars: 4,
    rules: { protectMain: false, requiredApprovals: 2, mergeRoles: 'editors' },
    members: members([
      ['u_alex', 'owner'],
      ['u_jordan', 'manager'],
      ['u_priya', 'editor'],
      ['u_sam', 'editor'],
      ['u_tomas', 'editor'],
      ['u_lena', 'approver'],
      ['u_mei', 'reviewer'],
    ]),
    labels: [
      { id: 'l_policy', name: 'policy', color: '6d28d9' },
      { id: 'l_budget', name: 'budget', color: '0f766e' },
      { id: 'l_question', name: 'question', color: 'b45309' },
    ],
  });
  handbookProject.issues = [
    { number: 3, title: 'Remote work section does not mention working from another country', body: 'Several people asked whether they can work from abroad for a few weeks. The handbook is silent on it.', state: 'open', authorId: 'u_tomas', assigneeId: 'u_jordan', labels: ['l_policy'], comments: [{ authorId: 'u_alex', at: now - 3 * DAY, text: 'Agreed. Up to four weeks a year seems to be the common answer.' }], createdAt: now - 6 * DAY },
    { number: 4, title: 'Training budget looks low for next year', body: 'Two courses are already booked for Q1. The plan line is 9,000.', state: 'open', authorId: 'u_lena', labels: ['l_budget', 'l_question'], comments: [], createdAt: now - 2 * DAY },
    { number: 2, title: 'Expenses deadline differs between handbook and finance page', body: 'Handbook said 60 days, finance said 30.', state: 'closed', authorId: 'u_priya', labels: ['l_policy'], comments: [{ authorId: 'u_priya', at: now - 26 * DAY, text: 'Fixed by #9.' }], createdAt: now - 30 * DAY, closedAt: now - 26 * DAY },
  ];

  const field = proj({
    id: 'p_field',
    name: 'Field guide',
    description: 'How the team runs site visits: safety, checklists and contacts.',
    role: 'editor',
    stars: 1,
    rules: { protectMain: true, requiredApprovals: 1, mergeRoles: 'owners' },
    members: members([
      ['u_alex', 'owner'],
      ['u_jordan', 'editor'],
      ['u_tomas', 'editor'],
      ['u_lena', 'approver'],
    ]),
    updatedAt: now - 4 * DAY,
  });
  const grant = proj({
    id: 'p_grant',
    name: 'Grant application',
    description: 'Narrative and budget for the community skills grant.',
    role: 'reviewer',
    ownerId: 'u_lena',
    rules: { protectMain: true, requiredApprovals: 1, mergeRoles: 'owners' },
    members: members([
      ['u_lena', 'owner'],
      ['u_jordan', 'reviewer'],
      ['u_priya', 'editor'],
    ]),
    space: 'sp_org',
    updatedAt: now - 8 * DAY,
  });

  const mkDoc = (id: string, projectId: string, title: string, filename: string, kind: FileKind, createdAt: number): Doc => {
    const d: Doc = { id, projectId, title, filename, kind, blocks: [], versions: [], connections: [], createdAt, discussions: [], copies: [], publicLinks: [] };
    db.docs.push(d);
    return d;
  };
  const rev = (d: Doc, ago: number, authorId: string, summary: string, changes: Change[], extra: { description?: string; kind?: 'merge'; mergedFrom?: string; branch?: string } = {}) => {
    const branch = extra.branch ?? 'main';
    const blocks = branch === 'main' ? d.blocks : db.drafts.find((x) => x.id === branch)!.blocks;
    return addCommit(db, { docId: d.id, branch, edits: edits(blocks, changes), summary, description: extra.description, authorId, at: now - ago, kind: extra.kind, mergedFrom: extra.mergedFrom });
  };
  const mkDraft = (id: string, d: Doc, name: string, authorId: string, ago: number, state: Draft['state'], baseSeq = mainSeq(db, d.id)): Draft => {
    const dr: Draft = { id, docId: d.id, projectId: d.projectId, name, authorId, baseSeq, createdAt: now - ago, state, blocks: d.blocks.map((b) => ({ ...b })) };
    db.drafts.push(dr);
    return dr;
  };
  const key = (host: AddinHost, createdAgo: number, lastUsedAgo?: number, revoked = false) => ({ id: `ck_${host}_${createdAgo}`, host, createdAt: now - createdAgo, expiresAt: now - createdAgo + 7 * DAY, revoked, lastUsed: lastUsedAgo === undefined ? undefined : now - lastUsedAgo });

  // ---- handbook.docx ----------------------------------------------------------------------
  const hb = mkDoc('d_handbook', 'p_handbook', 'Team handbook', 'handbook.docx', 'document', now - 60 * DAY);
  const P = (n: number, heading = false) => `¶ ${n}${heading ? ' · Heading' : ''}`;
  rev(hb, 60 * DAY, 'u_alex', 'Create the team handbook', [
    ['add', 'p1', P(1, true), '', 'Welcome to the team'],
    ['add', 'p2', P(2), 'p1', 'This handbook describes how we work together: our hours, how we plan, how we share documents and how we look after each other.'],
    ['add', 'p3', P(3, true), 'p2', 'Working hours'],
    ['add', 'p4', P(4), 'p3', 'Core hours are 10:00 to 16:00.'],
    ['add', 'p5', P(5, true), 'p4', 'Remote work'],
    ['add', 'p6', P(6), 'p5', 'You may work remotely up to two days a week. Agree the days with your manager at the start of each month.'],
    ['add', 'p8', P(8, true), 'p6', 'Leave'],
    ['add', 'p9', P(9), 'p8', 'Everyone has 22 days of annual leave plus public holidays.'],
    ['add', 'p10', P(10), 'p9', 'Parental leave is 12 weeks at full pay.'],
    ['add', 'p11', P(11, true), 'p10', 'Expenses'],
    ['add', 'p12', P(12), 'p11', 'Submit expenses within 60 days with a receipt. Travel is booked through the shared travel inbox.'],
  ]);
  rev(hb, 52 * DAY, 'u_priya', 'Shorten core hours', [['edit', 'p4', 'Core hours are 10:00 to 15:00 in your local time zone. Outside core hours, plan your day as suits you and your team.']], {
    description: 'Agreed in the September planning meeting.',
  });
  rev(hb, 45 * DAY, 'u_alex', 'Allow three remote days a week', [['edit', 'p6', 'You may work remotely up to three days a week. Agree the days with your manager at the start of each month.']]);
  rev(hb, 40 * DAY, 'u_sam', 'Add the monthly in-person day', [['add', 'p7', P(7), 'p6', 'The team meets in person on the first Tuesday of every month.']]);
  rev(hb, 33 * DAY, 'u_lena', 'Annual leave goes to 25 days', [['edit', 'p9', 'Everyone has 25 days of annual leave plus public holidays.']]);
  // Draft #9, merged.
  const d9 = mkDraft('dr_expenses', hb, 'expenses-deadline', 'u_priya', 29 * DAY, 'merged');
  rev(hb, 29 * DAY, 'u_priya', 'Expenses within 30 days', [['edit', 'p12', 'Submit expenses within 30 days with a receipt. Travel is booked through the shared travel inbox.']], { branch: d9.id });
  rev(hb, 26 * DAY, 'u_alex', 'Merge #9: Expenses within 30 days', [['edit', 'p12', 'Submit expenses within 30 days with a receipt. Travel is booked through the shared travel inbox.']], {
    kind: 'merge',
    mergedFrom: d9.id,
    description: 'Matches the finance page. Closes #2.',
  });
  rev(hb, 19 * DAY, 'u_alex', 'Add the documents and review section', [
    ['add', 'p13', P(13, true), 'p12', 'Documents And Review'],
    ['add', 'p14', P(14), 'p13', 'Shared documents live in Dynodoc. Edits to the handbook go through a change request and need two approvals.'],
  ]);
  // Draft #10, declined.
  const d10 = mkDraft('dr_fourday', hb, 'four-day-week', 'u_sam', 16 * DAY, 'withdrawn');
  rev(hb, 16 * DAY, 'u_sam', 'Trial a four-day week', [['edit', 'p4', 'From January the team trials a four-day week: core hours are 09:00 to 16:00, Monday to Thursday.']], { branch: d10.id });
  // Draft #14, open: parental leave (based on r7, so two revisions behind).
  const d14 = mkDraft('dr_parental', hb, 'parental-leave', 'u_priya', 9 * DAY, 'in_review');
  rev(hb, 9 * DAY, 'u_priya', 'Parental leave: 16 weeks for every parent', [['edit', 'p10', 'Parental leave is 16 weeks at full pay for every parent, whoever gave birth, and can be taken in up to three blocks.']], {
    branch: d14.id,
    description: 'Brings us in line with the policy the board approved.',
  });
  rev(hb, 6 * DAY, 'u_tomas', 'Say how to request leave', [['edit', 'p9', 'Everyone has 25 days of annual leave plus public holidays. Request leave at least two weeks ahead in the leave calendar.']]);
  rev(hb, 2 * DAY, 'u_priya', 'Fix heading case', [['edit', 'p13', 'Documents and review']]);
  // My draft: remote work, pushed, private.
  const mine = mkDraft('dr_remote', hb, 'remote-work-update', 'u_jordan', 1 * DAY, 'private');
  rev(hb, 1 * DAY, 'u_jordan', 'Allow working from another country', [
    ['edit', 'p6', 'You may work remotely up to three days a week, and from another country for up to four weeks a year. Agree the days with your manager at the start of each month.'],
  ], { branch: mine.id, description: 'Answers issue #3.' });
  hb.versions = [
    { id: 'v_hb2', name: 'Autumn 2026 edition', note: 'Shared with new starters from October.', throughSeq: 7, at: now - 19 * DAY, authorId: 'u_alex' },
    { id: 'v_hb1', name: 'First edition', note: '', throughSeq: 1, at: now - 60 * DAY, authorId: 'u_alex' },
  ];
  hb.connections = [key('word', 2 * DAY, 1 * HOUR), key('google-docs', 20 * DAY, 19 * DAY)];
  hb.discussions = [
    { id: 'th1', blockId: 'p7', authorId: 'u_tomas', text: 'Does the first Tuesday still work for the people in other time zones?', at: now - 5 * DAY, resolved: false, replies: [{ authorId: 'u_alex', text: 'We will check in the next planning meeting.', at: now - 4 * DAY }] },
    { id: 'th2', blockId: 'p4', authorId: 'u_mei', text: 'Core hours read well now.', at: now - 50 * DAY, resolved: true, replies: [] },
  ];
  hb.copies = [{ id: 'cp1', title: 'handbook (edited).docx', relation: 'edited_copy', score: 0.91, status: 'open' }];
  hb.publicLinks = [{ id: 'pl1', createdAt: now - 19 * DAY, expiresAt: now + 11 * DAY, revoked: false }];

  // ---- budget-2027.xlsx -------------------------------------------------------------------
  const bu = mkDoc('d_budget', 'p_handbook', 'Budget 2027', 'budget-2027.xlsx', 'spreadsheet', now - 40 * DAY);
  const C = (cell: string, what: string) => `Budget!${cell} · ${what}`;
  rev(bu, 40 * DAY, 'u_sam', 'Create the 2027 budget', [
    ['add', 'C4', C('C4', 'Salaries, plan'), '', '412,000'],
    ['add', 'D4', C('D4', 'Salaries, forecast'), 'C4', '409,500'],
    ['add', 'C5', C('C5', 'Contractors, plan'), 'D4', '48,000'],
    ['add', 'D5', C('D5', 'Contractors, forecast'), 'C5', '48,000'],
    ['add', 'C6', C('C6', 'Software, plan'), 'D5', '16,800'],
    ['add', 'D6', C('D6', 'Software, forecast'), 'C6', '18,600'],
    ['add', 'C7', C('C7', 'Travel, plan'), 'D6', '24,000'],
    ['add', 'D7', C('D7', 'Travel, forecast'), 'C7', '24,000'],
    ['add', 'C8', C('C8', 'Training, plan'), 'D7', '9,000'],
    ['add', 'D8', C('D8', 'Training, forecast'), 'C8', '9,000'],
    ['add', 'C9', C('C9', 'Office, plan'), 'D8', '30,000'],
    ['add', 'D9', C('D9', 'Office, forecast'), 'C9', '30,000'],
    ['add', 'C10', C('C10', 'Total, plan'), 'D9', '539,800  =SUM(C4:C9)'],
    ['add', 'D10', C('D10', 'Total, forecast'), 'C10', '539,100  =SUM(D4:D9)'],
  ]);
  rev(bu, 30 * DAY, 'u_lena', 'Contractors forecast up after Q2', [
    ['edit', 'D5', '52,500'],
    ['edit', 'D10', '543,600  =SUM(D4:D9)'],
  ]);
  const d11 = mkDraft('dr_software', bu, 'software-renewals', 'u_mei', 24 * DAY, 'merged');
  rev(bu, 24 * DAY, 'u_mei', 'Plan software renewals at the new price', [['edit', 'C6', '18,600'], ['edit', 'C10', '541,600  =SUM(C4:C9)']], { branch: d11.id });
  rev(bu, 21 * DAY, 'u_alex', 'Merge #11: Plan software renewals at the new price', [['edit', 'C6', '18,600'], ['edit', 'C10', '541,600  =SUM(C4:C9)']], { kind: 'merge', mergedFrom: d11.id });
  rev(bu, 14 * DAY, 'u_sam', 'Office forecast after the lease renewal', [['edit', 'D9', '31,200'], ['edit', 'D10', '544,800  =SUM(D4:D9)']]);
  // Draft #15, open, based on r4: conflicts with r5 on D7 and D10.
  const d15 = mkDraft('dr_travel', bu, 'travel-reforecast', 'u_sam', 8 * DAY, 'in_review');
  rev(bu, 8 * DAY, 'u_sam', 'Travel forecast for the January offsite', [['edit', 'D7', '26,800'], ['edit', 'D10', '547,600  =SUM(D4:D9)']], {
    branch: d15.id,
    description: 'Adds the offsite: 14 people, two nights.',
  });
  rev(bu, 5 * DAY, 'u_mei', 'Travel forecast down: fewer conference trips', [['edit', 'D7', '21,400'], ['edit', 'D10', '542,200  =SUM(D4:D9)']]);
  bu.versions = [{ id: 'v_bu1', name: 'Approved plan', note: 'As approved by the board.', throughSeq: 3, at: now - 21 * DAY, authorId: 'u_alex' }];
  bu.connections = [key('excel', 3 * DAY, 20 * MIN), key('google-sheets', 9 * DAY, undefined, true)];

  // ---- quarterly-review.pptx --------------------------------------------------------------
  const dk = mkDoc('d_deck', 'p_handbook', 'Quarterly review', 'quarterly-review.pptx', 'presentation', now - 20 * DAY);
  const S = (n: number, part: string) => `Slide ${n} · ${part}`;
  rev(dk, 20 * DAY, 'u_mei', 'Create the Q3 review deck', [
    ['add', 's1t', S(1, 'Title'), '', 'Quarterly review: Q3 2026'],
    ['add', 's1s', S(1, 'Subtitle'), 's1t', 'Team update, September 2026'],
    ['add', 's2t', S(2, 'Title'), 's1s', 'Highlights'],
    ['add', 's2b', S(2, 'Body'), 's2t', 'Handbook moved to Dynodoc · Budget forecast close to plan · Two new team members'],
    ['add', 's3t', S(3, 'Title'), 's2b', 'Budget'],
    ['add', 's3b', S(3, 'Body'), 's3t', 'Forecast 539,100 against a plan of 539,800.'],
    ['add', 's5t', S(5, 'Title'), 's3b', 'Questions'],
  ]);
  rev(dk, 9 * DAY, 'u_mei', 'Update the budget slide', [['edit', 's3b', 'Forecast 544,800 against a plan of 541,600. Contractors and office above plan; travel on plan.']]);
  rev(dk, 3 * DAY, 'u_alex', 'Add the next quarter slide', [
    ['add', 's4t', S(4, 'Title'), 's3b', 'Next quarter'],
    ['add', 's4b', S(4, 'Body'), 's4t', 'Finish the remote work policy · Review the training budget · Plan the January offsite'],
  ]);
  // Draft #16: an AI proposal from the agent key, open.
  const d16 = mkDraft('dr_agentdate', dk, 'fix-title-date', 'u_agent', 5 * HOUR, 'in_review');
  rev(dk, 5 * HOUR, 'u_agent', 'Fix the month on the title slide', [['edit', 's1s', 'Team update, October 2026']], { branch: d16.id });
  dk.connections = [key('powerpoint', 6 * DAY, 3 * DAY)];

  // ---- the other projects -------------------------------------------------------------------
  const fg = mkDoc('d_field', 'p_field', 'Field guide', 'field-guide.docx', 'document', now - 30 * DAY);
  rev(fg, 30 * DAY, 'u_alex', 'Create the field guide', [
    ['add', 'f1', P(1, true), '', 'Before a site visit'],
    ['add', 'f2', P(2), 'f1', 'Tell the site contact the day before. Check the weather and the access notes.'],
    ['add', 'f3', P(3, true), 'f2', 'On site'],
    ['add', 'f4', P(4), 'f3', 'Wear the safety kit. Visits are done in pairs.'],
    ['add', 'f5', P(5, true), 'f4', 'After the visit'],
    ['add', 'f6', P(6), 'f5', 'Upload notes and photos within two working days.'],
  ]);
  rev(fg, 12 * DAY, 'u_tomas', 'Add the check-in call', [['edit', 'f4', 'Wear the safety kit. Visits are done in pairs, with a check-in call to the office every three hours.']]);
  fg.connections = [key('google-docs', 4 * DAY, 4 * DAY)];

  const gn = mkDoc('d_grant', 'p_grant', 'Grant narrative', 'grant-narrative.docx', 'document', now - 15 * DAY);
  rev(gn, 15 * DAY, 'u_lena', 'First draft of the narrative', [
    ['add', 'g1', P(1, true), '', 'Community skills programme'],
    ['add', 'g2', P(2), 'g1', 'We will run twelve evening workshops on spreadsheets and budgeting for local volunteers.'],
    ['add', 'g3', P(3), 'g2', 'Each workshop takes up to 20 people. Materials are published under an open licence.'],
  ]);
  const gb = mkDoc('d_grantbudget', 'p_grant', 'Grant budget', 'grant-budget.xlsx', 'spreadsheet', now - 15 * DAY);
  rev(gb, 15 * DAY, 'u_lena', 'Grant budget lines', [
    ['add', 'B3', 'Lines!B3 · Trainers', '', '7,200'],
    ['add', 'B4', 'Lines!B4 · Venue', 'B3', '2,400'],
    ['add', 'B5', 'Lines!B5 · Materials', 'B4', '900'],
    ['add', 'B6', 'Lines!B6 · Total', 'B5', '10,500  =SUM(B3:B5)'],
  ]);
  rev(gb, 8 * DAY, 'u_priya', 'Venue at the community centre rate', [['edit', 'B4', '1,800'], ['edit', 'B6', '9,900  =SUM(B3:B5)']]);
  void field;
  void grant;

  // ---- change requests ----------------------------------------------------------------------
  const cr = (o: Partial<import('./types.ts').ChangeRequest> & Pick<import('./types.ts').ChangeRequest, 'id' | 'number' | 'docId' | 'draftId' | 'name' | 'authorId' | 'status' | 'createdAt'>) => {
    const d = db.drafts.find((x) => x.id === o.draftId)!;
    const r = {
      projectId: 'p_handbook',
      note: '',
      baseSeq: d.baseSeq,
      edits: draftEdits(db, d),
      excluded: [],
      conflicts: o.status === 'open' ? conflictsOf(db, d) : [],
      reviews: [],
      comments: [],
      ...o,
    };
    db.requests.push(r);
    return r;
  };
  // Closed ones keep the edits they had (the drafts' changes against their base).
  cr({ id: 'cr9', number: 9, docId: hb.id, draftId: d9.id, name: 'Expenses within 30 days', authorId: 'u_priya', status: 'merged', createdAt: now - 29 * DAY, mergedAt: now - 26 * DAY, note: 'Matches the finance page.', reviews: [
    { reviewerId: 'u_alex', verdict: 'approved', note: '', at: now - 27 * DAY, stale: false },
    { reviewerId: 'u_lena', verdict: 'approved', note: 'Thanks.', at: now - 26 * DAY, stale: false },
  ] });
  cr({ id: 'cr10', number: 10, docId: hb.id, draftId: d10.id, name: 'Trial a four-day week', authorId: 'u_sam', status: 'declined', createdAt: now - 16 * DAY, note: 'For discussion.', comments: [{ id: 'c10a', authorId: 'u_alex', text: 'Not this year; let us revisit in the spring.', at: now - 15 * DAY }] });
  cr({ id: 'cr11', number: 11, docId: bu.id, draftId: d11.id, name: 'Plan software renewals at the new price', authorId: 'u_mei', status: 'merged', createdAt: now - 24 * DAY, mergedAt: now - 21 * DAY, reviews: [
    { reviewerId: 'u_sam', verdict: 'approved', note: '', at: now - 22 * DAY, stale: false },
    { reviewerId: 'u_alex', verdict: 'approved', note: '', at: now - 21 * DAY, stale: false },
  ] });
  cr({
    id: 'cr14',
    number: 14,
    docId: hb.id,
    draftId: d14.id,
    name: 'Parental leave: 16 weeks for every parent',
    authorId: 'u_priya',
    status: 'open',
    createdAt: now - 9 * DAY,
    note: 'Brings the handbook in line with the policy the board approved on 2 October.',
    reviews: [
      { reviewerId: 'u_lena', verdict: 'commented', note: 'Can we say who to tell, and how early?', at: now - 3 * DAY, stale: false },
      { reviewerId: 'u_alex', verdict: 'approved', note: 'Looks right to me.', at: now - 1 * DAY, stale: false },
    ],
    comments: [
      { id: 'c14a', authorId: 'u_lena', blockId: 'p10', text: 'Can we say who to tell, and how early?', at: now - 3 * DAY },
      { id: 'c14b', authorId: 'u_priya', blockId: 'p10', text: 'Good point: I will add it in a separate change so this one stays small.', at: now - 2 * DAY },
    ],
  });
  cr({ id: 'cr15', number: 15, docId: bu.id, draftId: d15.id, name: 'Travel forecast for the January offsite', authorId: 'u_sam', status: 'open', createdAt: now - 3 * HOUR, note: 'Adds the offsite: 14 people, two nights.' });
  cr({
    id: 'cr16',
    number: 16,
    docId: dk.id,
    draftId: d16.id,
    name: 'Fix the month on the title slide',
    authorId: 'u_agent',
    status: 'open',
    createdAt: now - 5 * HOUR,
    byAgent: true,
    agentStatement:
      'The subtitle says September, but the deck covers the quarter that ended on 30 September and is presented in October. I changed the month to October. I did not check the figures on the other slides.',
  });

  // ---- captured changes on this computer (not yet committed) --------------------------------
  db.working = [
    { id: 'w1', docId: hb.id, branch: 'main', staged: true, source: 'word', capturedAt: now - 40 * MIN, edit: { blockId: 'p12', op: 'edit', before: hb.blocks.find((b) => b.id === 'p12')!.text, after: 'Submit expenses within 30 days with a receipt. Travel is booked through the shared travel inbox at least ten days ahead.' } },
    { id: 'w2', docId: hb.id, branch: 'main', staged: false, source: 'word', capturedAt: now - 35 * MIN, edit: { blockId: 'p2', op: 'edit', before: hb.blocks.find((b) => b.id === 'p2')!.text, after: 'This handbook describes how we work together: our hours, how we plan, how we share documents, and how we look after one another.' } },
    { id: 'w3', docId: bu.id, branch: 'main', staged: true, source: 'excel', capturedAt: now - 22 * MIN, edit: { blockId: 'C8', op: 'edit', before: '9,000', after: '12,000' } },
    { id: 'w4', docId: bu.id, branch: 'main', staged: true, source: 'excel', capturedAt: now - 22 * MIN, edit: { blockId: 'C10', op: 'edit', before: bu.blocks.find((b) => b.id === 'C10')!.text, after: '544,600  =SUM(C4:C9)' } },
    { id: 'w5', docId: dk.id, branch: 'main', staged: false, source: 'powerpoint', capturedAt: now - 9 * MIN, edit: { blockId: 's4b', op: 'edit', before: dk.blocks.find((b) => b.id === 's4b')!.text, after: 'Finish the remote work policy · Review the training budget (+3,000 in the plan) · Plan the January offsite' } },
    { id: 'w6', docId: dk.id, branch: 'main', staged: false, source: 'powerpoint', capturedAt: now - 8 * MIN, edit: { blockId: 's4n', op: 'add', afterId: 's4b', label: 'Slide 4 · Notes', after: 'Ask Sam for the offsite numbers before the meeting.' } },
    { id: 'w7', docId: hb.id, branch: mine.id, staged: true, source: 'word', capturedAt: now - 15 * MIN, edit: { blockId: 'p7', op: 'edit', before: hb.blocks.find((b) => b.id === 'p7')!.text, after: 'The team meets in person on the first Tuesday of every month. Travel to it is paid from the team budget.' } },
  ];

  // ---- notifications and activity -------------------------------------------------------------
  db.notifications = [
    { id: 'n1', kind: 'change_request_submitted', at: now - 3 * HOUR, read: false, actorId: 'u_sam', projectId: 'p_handbook', docId: bu.id, requestId: 'cr15', text: 'opened #15 Travel forecast for the January offsite' },
    { id: 'n2', kind: 'change_request_submitted', at: now - 5 * HOUR, read: false, actorId: 'u_agent', projectId: 'p_handbook', docId: dk.id, requestId: 'cr16', text: 'proposed #16 Fix the month on the title slide' },
    { id: 'n3', kind: 'change_request_reviewed', at: now - 1 * DAY, read: false, actorId: 'u_alex', projectId: 'p_handbook', docId: hb.id, requestId: 'cr14', text: 'approved #14 Parental leave: 16 weeks for every parent' },
    { id: 'n4', kind: 'copy_detected', at: now - 2 * DAY, read: true, actorId: 'u_mei', projectId: 'p_handbook', docId: hb.id, text: 'uploaded handbook (edited).docx, an edited copy of Team handbook (91% similar)' },
    { id: 'n5', kind: 'access_granted', at: now - 8 * DAY, read: true, actorId: 'u_lena', projectId: 'p_grant', text: 'gave you Contributor access to Grant application' },
    { id: 'n6', kind: 'change_request_merged', at: now - 21 * DAY, read: true, actorId: 'u_alex', projectId: 'p_handbook', docId: bu.id, requestId: 'cr11', text: 'merged #11 Plan software renewals at the new price' },
  ];
  const act = (ago: number, actorId: string, projectId: string, kind: import('./types.ts').ActivityItem['kind'], summary: string, docId?: string, byAgent = false) =>
    db.activity.push({ id: `a${db.activity.length}`, at: now - ago, actorId, projectId, kind, summary, docId, byAgent });
  act(5 * HOUR, 'u_agent', 'p_handbook', 'request', 'proposed #16 Fix the month on the title slide', dk.id, true);
  act(3 * HOUR, 'u_sam', 'p_handbook', 'request', 'opened #15 Travel forecast for the January offsite', bu.id);
  act(1 * DAY, 'u_jordan', 'p_handbook', 'draft', 'pushed remote-work-update (1 commit)', hb.id);
  act(1 * DAY, 'u_alex', 'p_handbook', 'review', 'approved #14 Parental leave: 16 weeks for every parent', hb.id);
  act(2 * DAY, 'u_priya', 'p_handbook', 'commit', 'committed "Fix heading case" to main (r9)', hb.id);
  act(3 * DAY, 'u_alex', 'p_handbook', 'commit', 'committed "Add the next quarter slide" to main (r3)', dk.id);
  act(5 * DAY, 'u_mei', 'p_handbook', 'commit', 'committed "Travel forecast down: fewer conference trips" to main (r5)', bu.id);
  act(6 * DAY, 'u_tomas', 'p_handbook', 'issue', 'opened issue #3 Remote work section does not mention working from another country');
  act(6 * DAY, 'u_tomas', 'p_handbook', 'commit', 'committed "Say how to request leave" to main', hb.id);
  act(8 * DAY, 'u_priya', 'p_grant', 'commit', 'committed "Venue at the community centre rate" to main', gb.id);
  act(12 * DAY, 'u_tomas', 'p_field', 'commit', 'committed "Add the check-in call" to main', fg.id);
  act(19 * DAY, 'u_alex', 'p_handbook', 'version', 'saved the version "Autumn 2026 edition" of Team handbook', hb.id);
  act(21 * DAY, 'u_alex', 'p_handbook', 'merge', 'merged #11 Plan software renewals at the new price', bu.id);
  db.activity.sort((a, b) => b.at - a.at);

  return db;
}

/** Things a Fetch brings in, one per fetch, so the sample moves when a person syncs. */
export const INCOMING: { summary: string; apply: (db: Db) => string }[] = [
  {
    summary: 'Lena Fischer approved #14',
    apply: (db) => {
      const r = db.requests.find((x) => x.id === 'cr14');
      if (r && r.status === 'open') {
        r.reviews.push({ reviewerId: 'u_lena', verdict: 'approved', note: 'Fine as it is; the notice period can follow.', at: Date.now(), stale: false });
        db.notifications.unshift({ id: `n_in${Date.now()}`, kind: 'change_request_reviewed', at: Date.now(), read: false, actorId: 'u_lena', projectId: 'p_handbook', docId: 'd_handbook', requestId: 'cr14', text: 'approved #14 Parental leave: 16 weeks for every parent' });
      }
      return 'Lena Fischer approved #14. It now has the two approvals it needs.';
    },
  },
  {
    summary: 'Tomás Ruiz committed to main',
    apply: (db) => {
      const d = db.docs.find((x) => x.id === 'd_handbook')!;
      const p = d.blocks.find((b) => b.id === 'p14');
      if (p) {
        addCommit(db, {
          docId: d.id,
          branch: 'main',
          edits: [{ blockId: 'p14', op: 'edit', before: p.text, after: 'Shared documents live in Dynodoc. Edits to the handbook go through a change request and need two approvals; anyone may comment.' }],
          summary: 'Say that anyone may comment',
          authorId: 'u_tomas',
        });
      }
      return 'Tomás Ruiz committed "Say that anyone may comment" to main. Your drafts of the handbook are one revision further behind.';
    },
  },
];
