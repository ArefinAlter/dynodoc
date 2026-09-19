// The homepage FAQ, in one place. The visible <details> list and the FAQPage
// structured data both render from this array, so the marked-up answers can
// never say something different from the answers a visitor reads — which is
// both a structured-data requirement and the point of the markup.

export type FaqEntry = { question: string; answer: string };

export const FAQ: FaqEntry[] = [
  {
    question: 'What makes Dynodoc useful for research teams?',
    answer:
      'Documents, spreadsheets and questionnaires share the same approach to drafts, review and version history. You can improve an instrument or prepare a dataset while keeping a record of the content changes. It is especially useful when several people need to propose revisions before they become part of the team’s version.'
  },
  {
    question: 'Will the editors feel familiar?',
    answer:
      'The document editor provides common formatting, headings, lists, links, images, tables and comments. The spreadsheet editor includes formulas, multiple sheets, cell formatting and row and column tools. The pilot does not yet match every feature of Google Docs, Google Sheets, Word or Excel; advanced charts, pivots and citation tools are not included.'
  },
  {
    question: 'Can we work without learning Git?',
    answer:
      'Yes. You use actions such as “Save version,” “Create draft,” “Review changes” and “Share changes.” Drafts let you try a direction; comparison lets you see the differences; sharing brings selected changes into the team version. When edits overlap, the interface asks you which content to keep.'
  },
  {
    question: 'Can everyone type in the same document at once?',
    answer:
      'The current pilot centers on personal drafts and deliberate sharing. It does not provide live character-by-character co-editing or live collaborator cursors. Get the latest shared changes into your draft, resolve any overlaps and share back when ready.'
  },
  {
    question: 'How do imports and research integrations work?',
    answer:
      'Upload DOCX, legacy DOC, XLSX or CSV files. For ODK Central and KoboToolbox, you can also connect to your server and import an available XLSForm or a response snapshot. Other listed tools use compatible file exports. Imports are snapshots, with no scheduled sync or writeback. Review conversion warnings, formulas and survey logic before relying on imported material.'
  },
  {
    question: 'Is it ready for our next study?',
    answer:
      'Dynodoc is a research pilot being prepared for a small team. Start with sample or de-identified materials and validate your workflow, import fidelity and access arrangements. The current pilot is not a claim of institutional compliance, a regulated-data hosting agreement or a full replacement for specialist collection and analysis tools.'
  },
  {
    question: 'Can we take our work with us?',
    answer:
      'You can export supported documents and spreadsheets and retrieve retained original uploads. Check the exported result when your files use complex formatting or formulas. Dynodoc’s application code is open source under the AGPL; your private research content does not become open source because you use the app.'
  }
];
