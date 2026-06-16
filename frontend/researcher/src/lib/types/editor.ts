export type EditorMode = 'document' | 'outline' | 'spec' | 'preview';

export interface EditorChoice {
  id: string;
  label: string;
}

export interface EditorItem {
  id: string;
  number: string;
  type: string;
  text: string;
  hint?: string;
  varName?: string;
  relevance?: string;
  choices?: EditorChoice[];
  commentsCount?: number;
  suggestionsCount?: number;
}

export interface EditorSection {
  id: string;
  number: string;
  title: string;
  meta?: string;
  moduleTag?: string;
  moduleName?: string;
  moduleCitation?: string;
  items: EditorItem[];
}

export interface ActivityEvent {
  id: string;
  time: string;
  actor: string;
  action: string;
  target: string;
}

export interface ConflictOption {
  id: string;
  label: string;
  text: string;
}

export interface ConflictCard {
  id: string;
  itemId: string;
  itemLabel: string;
  remaining: number;
  options: ConflictOption[];
}

export interface EditorDocument {
  id: string;
  title: string;
  overline: string;
  organization: string;
  version: string;
  status: string;
  languages: string[];
  collaborators: string[];
  sections: EditorSection[];
  activity: ActivityEvent[];
  conflict: ConflictCard;
}
