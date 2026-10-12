/** Props shared by every mockup component. */
export interface MockProps {
  /** Play the scroll-into-view motion. The final state is rendered when false, with JS off, or under reduced motion. */
  animate?: boolean;
  /** Accessible name. Omit for decorative use: the mockup is then `aria-hidden`. */
  label?: string;
  class?: string;
}
