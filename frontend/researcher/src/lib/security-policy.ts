export function isOfficePane(pathname: string) {
  return /^\/connectors\/(word|excel|powerpoint)\/?$/.test(pathname);
}
