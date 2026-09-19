export interface MenuAction {
  label: string;
  action: () => void | Promise<void>;
  disabled?: boolean;
  separator?: boolean;
}
