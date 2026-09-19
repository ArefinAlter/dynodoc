export interface Space {
  id: string;
  parent_id: string | null;
  name: string;
  kind: string;
  role: string;
}
