import type { EditorDocument } from '$lib/types/editor';

export const sampleDocument: EditorDocument = {
  id: 'demo-phq9',
  title: 'Household Health And Wellbeing Instrument',
  overline: 'Research IDE PoC · Editor scaffold',
  organization: 'Verne Ltd. · Internal methods team',
  version: 'draft v0.3',
  status: 'Draft',
  languages: ['en'],
  collaborators: ['Verne', 'Aluk', 'Dr. Rahman'],
  sections: [
    {
      id: 'sec-intro',
      number: '1',
      title: 'Introduction',
      meta: 'Enumerator intro',
      items: [
        {
          id: 'item-intro-1',
          number: '1.1',
          type: 'paragraph',
          text: 'Read the introduction exactly as written before beginning the module.',
          hint: 'Visible in preview as enumerator guidance.',
          varName: 'intro_read',
          commentsCount: 1
        }
      ]
    },
    {
      id: 'sec-phq9',
      number: '2',
      title: 'Depression Screening',
      meta: 'Validated module',
      moduleTag: 'MODULE',
      moduleName: 'PHQ-9',
      moduleCitation: 'Validated short-form instrument',
      items: [
        {
          id: 'item-q1',
          number: '2.1',
          type: 'select_one',
          text: 'Over the last two weeks, how often have you been bothered by little interest or pleasure in doing things?',
          hint: 'Use validated wording.',
          varName: 'phq_interest',
          choices: [
            { id: 'c0', label: 'Not at all' },
            { id: 'c1', label: 'Several days' },
            { id: 'c2', label: 'More than half the days' },
            { id: 'c3', label: 'Nearly every day' }
          ],
          commentsCount: 2,
          suggestionsCount: 1
        },
        {
          id: 'item-q2',
          number: '2.2',
          type: 'select_one',
          text: 'Over the last two weeks, how often have you been bothered by feeling down, depressed, or hopeless?',
          varName: 'phq_down',
          choices: [
            { id: 'c0', label: 'Not at all' },
            { id: 'c1', label: 'Several days' },
            { id: 'c2', label: 'More than half the days' },
            { id: 'c3', label: 'Nearly every day' }
          ],
          commentsCount: 1
        },
        {
          id: 'item-q3',
          number: '2.3',
          type: 'integer',
          text: 'How many hours did you spend outdoors yesterday?',
          hint: 'Numeric whole hours only.',
          varName: 'hours_outdoors',
          relevance: '${phq_interest} != "c0"',
          commentsCount: 0,
          suggestionsCount: 1
        }
      ]
    }
  ],
  activity: [
    {
      id: 'a1',
      time: '12m ago',
      actor: 'Aluk',
      action: 'suggested rewording on',
      target: '2.3 — Hours outdoors'
    },
    {
      id: 'a2',
      time: '37m ago',
      actor: 'Dr. Rahman',
      action: 'added comment on',
      target: '2.1 — Little interest'
    },
    {
      id: 'a3',
      time: '2h ago',
      actor: 'Verne',
      action: 'created',
      target: 'draft v0.3'
    }
  ],
  conflict: {
    id: 'conflict-1',
    itemId: 'item-q3',
    itemLabel: '2.3 — Hours outdoors',
    remaining: 5,
    options: [
      {
        id: 'original',
        label: 'Original',
        text: 'How many hours did you spend outdoors yesterday?'
      },
      {
        id: 'aluk',
        label: 'Aluk',
        text: 'Yesterday, for approximately how many hours were you outside the home?'
      },
      {
        id: 'rahman',
        label: 'Dr. Rahman',
        text: 'Yesterday, how many hours in total did you spend outdoors, including work and travel?'
      },
      {
        id: 'compose',
        label: 'Compose new',
        text: 'Yesterday, about how many total hours did you spend outdoors?'
      }
    ]
  }
};
