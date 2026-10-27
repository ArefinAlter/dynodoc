// The homepage FAQ, in one place. The visible <details> list and the FAQPage
// structured data both render from this array, so the marked-up answers can
// never say something different from the answers a visitor reads — which is
// both a structured-data requirement and the point of the markup.

export type FaqEntry = { question: string; answer: string };

export const FAQ: FaqEntry[] = [
  {
    question: 'What is Dynodoc?',
    answer:
      'Dynodoc is version control for documents written in Word, Google Docs, Excel, Google Sheets, PowerPoint and Google Slides. It records changes against stable paragraph, cell and slide identities, routes them through change requests and review, and keeps an append-only, hash-chained history that can be verified. Editing stays in those editors.'
  },
  {
    question: 'How is it different from track changes or version history?',
    answer:
      'Track changes and built-in version history live inside one file in one editor. Dynodoc keeps one history across files and editors: a project with private drafts, change requests with approval rules, named versions and a hash-chained log of who changed what and when. A change made in Excel and a change made in Word are reviewed in the same place.'
  },
  {
    question: 'Which editors and file types does it work with?',
    answer:
      'Word, Excel and PowerPoint through task-pane add-ins, and Google Docs, Sheets and Slides through Apps Script add-ons. You can add DOCX, XLSX, PPTX, CSV and Markdown files to a project. Dynodoc also has basic web editors for documents and spreadsheets, which are not the main product.'
  },
  {
    question: 'How does capture work?',
    answer:
      'An add-in watches the open file while its pane is open and records edits to existing paragraph text, cell values and formulas, and slide text. There is no background agent, so edits made while the pane is closed are picked up by pushing the edited file instead. A reviewer sees what was captured before anything merges.'
  },
  {
    question: 'Is the history tamper-evident?',
    answer:
      'Every event is appended and hashed together with the one before it, so altering an earlier event breaks every later hash, and anyone with access can verify the chain. That is tamper-evident, not tamper-proof: spotting a wholesale rewrite by a database administrator also needs a hash or backup you kept independently. External witnesses are planned, not shipped.'
  },
  {
    question: 'Can I self-host it?',
    answer:
      'Partly. The engine, HTTP API and command-line tools are open source under the MIT licence. The engine repository explains how to run them against PostgreSQL 16 with Docker, Rust and Task: task up, task migrate, task db:grant-runtime and task run. The workspace and website are not published as open source, so the full hosted application cannot be self-hosted from public source. Private document content you put in the hosted app does not become open source.'
  },
  {
    question: 'What does it not do?',
    answer:
      'There is no character-by-character co-editing and no live collaborator cursors; collaboration works through drafts, change requests and deliberate merges. The add-ins do not run in the background and do not capture every keystroke, layout change or embedded object. Dynodoc is not a replacement editor for Office or Google, and it makes no compliance claims.'
  },
  {
    question: 'What does it cost, and who can use it?',
    answer:
      'There is no paid plan and no payment flow yet. The hosted workspace at app.dynodoc.online is a test deployment with email and Google sign-in, and projects are invite-only: only their members can see them. The 0.0.1b pre-alpha lets you start a sample workspace without an account (removed after 24 hours) and download an unsigned desktop app for Windows and Linux that keeps your workspace on your own computer. The add-ins are test installs, not marketplace listings. The engine, API and command-line tools are open source, so you can run those yourself today. The terms and privacy policy are drafts and are not yet effective.'
  }
];
