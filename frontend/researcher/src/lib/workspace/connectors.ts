export type ResearchProvider = 'odk' | 'kobo';
export interface ResearchSource {
  provider: ResearchProvider;
  origin: string;
  project: string;
  form: string;
  kind: 'questionnaire' | 'submissions';
  fetched_at: string;
}
export interface ResearchRequest {
  provider: ResearchProvider;
  origin: string;
  token: string;
  action: 'projects' | 'forms' | 'import';
  project?: string;
  form?: string;
  kind?: 'questionnaire' | 'submissions';
}
export interface ResearchFile {
  filename: string;
  content_type: string;
  content_base64: string;
  warnings: string[];
  source: ResearchSource;
}
export interface ResearchChoice {
  id: string;
  name: string;
}
