// LaTeX and Overleaf imports are not part of the desktop client: the page asks for Word, Excel and
// PowerPoint files only.
export function importLatex(): never {
  throw new Error('LaTeX files are not supported in the desktop app.');
}
export function importOverleaf(): never {
  throw new Error('Overleaf projects are not supported in the desktop app.');
}
