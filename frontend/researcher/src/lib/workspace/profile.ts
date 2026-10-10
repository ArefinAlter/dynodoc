import { workspaceRequest } from './api';
export interface Profile {
  id: string;
  name: string;
  bio: string;
  revision: number;
}
export const loadProfile = (id?: string) =>
  workspaceRequest<Profile>(id ? `/profiles/${id}` : '/profile');
export const initials = (name: string) =>
  name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0] || '')
    .join('')
    .toUpperCase() || 'D';
