import { mkdir, writeFile } from 'node:fs/promises';
import ExcelJS from 'exceljs';
import {
  Document,
  Packer,
  Paragraph,
  TextRun,
  HeadingLevel,
  Table,
  TableRow,
  TableCell
} from 'docx';
const destination = new URL('../static/examples/', import.meta.url);
await mkdir(destination, { recursive: true });
const doc = new Document({
  sections: [
    {
      children: [
        new Paragraph({ text: 'Community wellbeing study', heading: HeadingLevel.HEADING_1 }),
        new Paragraph({
          children: [
            new TextRun({ text: 'Example interview guide', bold: true }),
            new TextRun(' · Synthetic training material')
          ]
        }),
        new Paragraph(
          'Use this example to practice editing, sharing a personal draft, and reviewing changes. It is not an approved research protocol.'
        ),
        new Paragraph({ text: '1. Opening the conversation', heading: HeadingLevel.HEADING_2 }),
        new Paragraph(
          'Thank you for speaking with us today. We would like to understand everyday life in your community.'
        ),
        new Paragraph('What has changed in your neighbourhood over the past year?'),
        new Paragraph({ text: '2. Access to services', heading: HeadingLevel.HEADING_2 }),
        new Paragraph('Tell us about a time you needed healthcare in the past six months.'),
        new Table({
          rows: [
            ['Topic', 'Follow-up prompt'],
            ['Travel', 'How did you get there?'],
            ['Cost', 'Were any costs difficult to manage?']
          ].map(
            (row) =>
              new TableRow({
                children: row.map((text) => new TableCell({ children: [new Paragraph(text)] }))
              })
          )
        }),
        new Paragraph({ text: 'Research team notes', heading: HeadingLevel.HEADING_2 }),
        new Paragraph(
          'Add a comment to this section, then save a named version before field testing.'
        )
      ]
    }
  ]
});
await writeFile(new URL('community-interview-guide.docx', destination), await Packer.toBuffer(doc));
const tracker = new ExcelJS.Workbook(),
  sheet = tracker.addWorksheet('Fieldwork');
sheet.addRows([
  ['Site', 'Planned', 'Completed', 'Remaining'],
  ['Dhaka', 20, 12, { formula: 'B2-C2', result: 8 }],
  ['Sylhet', 15, 8, { formula: 'B3-C3', result: 7 }],
  ['Rajshahi', 10, 6, { formula: 'B4-C4', result: 4 }],
  [
    'Total',
    { formula: 'SUM(B2:B4)', result: 45 },
    { formula: 'SUM(C2:C4)', result: 26 },
    { formula: 'SUM(D2:D4)', result: 19 }
  ]
]);
sheet.columns.forEach((c) => (c.width = 22));
sheet.getRow(1).font = { bold: true };
await tracker.xlsx.writeFile(new URL('fieldwork-tracker.xlsx', destination).pathname);
const form = new ExcelJS.Workbook();
form.addWorksheet('survey').addRows([
  ['type', 'name', 'label', 'hint', 'required', 'relevant', 'constraint'],
  ['begin_group', 'background', 'Background', '', '', '', ''],
  [
    'integer',
    'age',
    'How old are you?',
    'Age in completed years',
    'yes',
    '',
    '. >= 18 and . <= 120'
  ],
  ['select_one yesno', 'consent', 'May we continue with the interview?', '', 'yes', '', ''],
  [
    'text',
    'community',
    'What is one thing you value about your community?',
    '',
    '',
    "${consent} = '1'",
    ''
  ],
  ['end_group', '', '', '', '', '', '']
]);
form.addWorksheet('choices').addRows([
  ['list_name', 'name', 'label'],
  ['yesno', '1', 'Yes'],
  ['yesno', '0', 'No']
]);
form.addWorksheet('settings').addRows([
  ['form_title', 'form_id', 'version'],
  ['Example community questionnaire', 'dynodoc_example_community', '1']
]);
await form.xlsx.writeFile(new URL('community-questionnaire.xlsx', destination).pathname);
await writeFile(
  new URL('interview-tracker.csv', destination),
  'Site,Planned,Completed\nDhaka,20,12\nSylhet,15,8\nRajshahi,10,6\n'
);
console.log('Generated four synthetic example files.');
