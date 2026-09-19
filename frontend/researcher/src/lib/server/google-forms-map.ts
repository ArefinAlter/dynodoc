import Papa from 'papaparse';
export interface Choice {
  type?: string;
  options?: {
    value?: string;
    isOther?: boolean;
    goToAction?: string;
    goToSectionId?: string;
    image?: unknown;
  }[];
  shuffle?: boolean;
}
export interface Question {
  questionId?: string;
  required?: boolean;
  choiceQuestion?: Choice;
  textQuestion?: { paragraph?: boolean };
  scaleQuestion?: { low: number; high: number; lowLabel?: string; highLabel?: string };
  ratingQuestion?: { ratingScaleLevel?: number; iconType?: string };
  dateQuestion?: { includeTime?: boolean; includeYear?: boolean };
  timeQuestion?: { duration?: boolean };
  fileUploadQuestion?: unknown;
  rowQuestion?: { title?: string };
  grading?: unknown;
}
export interface GoogleForm {
  formId: string;
  info?: { title?: string; description?: string };
  items?: {
    itemId?: string;
    title?: string;
    description?: string;
    questionItem?: { question: Question; image?: unknown };
    questionGroupItem?: {
      questions: Question[];
      grid?: { columns: Choice; shuffleQuestions?: boolean };
    };
    pageBreakItem?: unknown;
    imageItem?: unknown;
    videoItem?: unknown;
  }[];
  settings?: { quizSettings?: { isQuiz?: boolean } };
}
export interface GoogleResponse {
  responseId?: string;
  createTime?: string;
  lastSubmittedTime?: string;
  respondentEmail?: string;
  answers?: Record<
    string,
    {
      textAnswers?: { answers?: { value?: string }[] };
      fileUploadAnswers?: { answers?: unknown[] };
    }
  >;
}
export function formId(value: string) {
  let id = value.trim();
  if (id.startsWith('https://')) {
    const u = new URL(id);
    if (u.hostname !== 'docs.google.com')
      throw new Error('Use the Google Forms editor URL or form ID.');
    const m = u.pathname.match(/^\/forms\/d\/([A-Za-z0-9_-]+)(?:\/|$)/);
    id = m?.[1] || '';
  }
  if (!/^[A-Za-z0-9_-]{10,200}$/.test(id) || id === 'e')
    throw new Error('Use the editor form ID, not a forms.gle link or published responder URL.');
  return id;
}
export function questionnaireRows(form: GoogleForm) {
  const survey: Record<string, string>[] = [];
  const choices: Record<string, string>[] = [];
  const warnings = new Set<string>([
    'Google Forms import creates an independent copy. Review logic and validate the XLSForm before field use.'
  ]);
  const names = new Set<string>();
  function question(q: Question, label: string, hint = '', grid?: Choice) {
    const name = 'q_' + String(q.questionId || survey.length).replace(/[^A-Za-z0-9_]/g, '_');
    if (names.has(name)) throw new Error('The form contains duplicate question IDs.');
    names.add(name);
    const row: Record<string, string> = {
      name,
      label,
      hint,
      type: 'text',
      required: q.required ? 'yes' : 'no',
      google_question_id: q.questionId || ''
    };
    const choice = q.choiceQuestion || grid;
    if (choice) {
      row.type = (choice.type === 'CHECKBOX' ? 'select_multiple ' : 'select_one ') + name;
      if (choice.type === 'DROP_DOWN') row.appearance = 'minimal';
      choice.options?.forEach((o, i) => {
        choices.push({ list_name: name, name: `option_${i + 1}`, label: o.value || 'Other' });
        if (o.isOther)
          warnings.add('Free-text Other options require a follow-up text question after import.');
        if (o.goToAction || o.goToSectionId)
          warnings.add(
            'Google Forms section branching is retained in the source JSON and must be recreated as relevance expressions.'
          );
        if (o.image)
          warnings.add(
            'Choice images are retained in source JSON; media files are not downloaded.'
          );
      });
      if (choice.shuffle) row.parameters = 'randomize=true';
    } else if (q.scaleQuestion || q.ratingQuestion) {
      const scale = q.scaleQuestion;
      const low = scale?.low ?? 1,
        high = scale?.high ?? q.ratingQuestion?.ratingScaleLevel ?? 5;
      row.type = 'integer';
      row.constraint = `. >= ${low} and . <= ${high}`;
      row.hint = [hint, scale?.lowLabel, scale?.highLabel].filter(Boolean).join(' · ');
      row.appearance = '';
    } else if (q.dateQuestion) {
      row.type = q.dateQuestion.includeTime ? 'dateTime' : 'date';
      if (q.dateQuestion.includeYear === false)
        warnings.add('Dates without a year need review: the imported date field includes a year.');
    } else if (q.timeQuestion) {
      row.type = q.timeQuestion.duration ? 'text' : 'time';
      if (q.timeQuestion.duration)
        row.hint = [hint, 'Duration (hours:minutes:seconds)'].filter(Boolean).join(' · ');
    } else if (q.fileUploadQuestion) {
      row.type = 'file';
      warnings.add(
        'File-upload question definitions are imported. Uploaded file contents and Google Drive permissions are not transferred.'
      );
    } else if (q.textQuestion?.paragraph) row.appearance = 'multiline';
    else if (!q.textQuestion) {
      warnings.add(
        'An unsupported question was retained as text; inspect the original Google Forms source.'
      );
    }
    if (q.grading)
      warnings.add('Quiz grading is preserved in source JSON but does not run in Dynodoc.');
    survey.push(row);
  }
  if (form.info?.description)
    survey.push({ type: 'note', name: 'form_description', label: form.info.description });
  for (const item of form.items || []) {
    if (item.questionItem) {
      question(item.questionItem.question, item.title || 'Untitled question', item.description);
      if (item.questionItem.image)
        warnings.add(
          'Question images are retained in source JSON; media files are not downloaded.'
        );
    } else if (item.questionGroupItem) {
      for (const q of item.questionGroupItem.questions)
        question(
          q,
          [item.title, q.rowQuestion?.title].filter(Boolean).join(' — '),
          item.description,
          item.questionGroupItem.grid?.columns
        );
      warnings.add(
        'Grid questions are represented as individual questions, with stable Google question IDs.'
      );
    } else {
      survey.push({
        type: 'note',
        name: `note_${survey.length}`,
        label: item.title || 'Section',
        hint: item.description || ''
      });
      if (item.imageItem || item.videoItem)
        warnings.add('Embedded media is retained in source JSON; media files are not downloaded.');
    }
  }
  if (survey.length > 2000)
    throw new Error('Split forms larger than 2,000 items before importing.');
  return { survey, choices, warnings: [...warnings] };
}
export function responseCsv(form: GoogleForm, responses: GoogleResponse[]) {
  const ids = new Map<string, string>();
  for (const item of form.items || []) {
    if (item.questionItem?.question.questionId)
      ids.set(item.questionItem.question.questionId, item.title || 'Question');
    for (const q of item.questionGroupItem?.questions || [])
      if (q.questionId)
        ids.set(q.questionId, [item.title, q.rowQuestion?.title].filter(Boolean).join(' / '));
  }
  for (const response of responses)
    for (const id of Object.keys(response.answers || {}))
      if (!ids.has(id)) ids.set(id, 'Earlier question');
  if (ids.size > 96)
    throw new Error(
      'This response table exceeds 100 columns. Export selected columns from Google Sheets instead.'
    );
  const columns = [
    'response_id',
    'created_at',
    'submitted_at',
    'respondent_email',
    ...[...ids].map(([id, label]) => `${label} [${id}]`)
  ];
  const rows = responses.map((r) => [
    r.responseId || '',
    r.createTime || '',
    r.lastSubmittedTime || '',
    r.respondentEmail || '',
    ...[...ids.keys()].map((id) => {
      const a = r.answers?.[id];
      if (a?.fileUploadAnswers) return JSON.stringify(a.fileUploadAnswers.answers || []);
      const values = a?.textAnswers?.answers?.map((x) => x.value || '') || [];
      return values.length > 1 ? JSON.stringify(values) : values[0] || '';
    })
  ]);
  // Formula-looking response text remains literal when the CSV is opened in a spreadsheet.
  return Papa.unparse({ fields: columns, data: rows }, { escapeFormulae: true });
}
