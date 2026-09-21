declare module 'pagedjs' {
  export class Previewer {
    preview(
      content: DocumentFragment | HTMLElement | string,
      stylesheets: (string | Record<string, string>)[],
      renderTo: HTMLElement
    ): Promise<{ total: number }>;
  }
}
