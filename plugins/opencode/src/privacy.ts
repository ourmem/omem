export function stripPrivateContent(text: string): string {
  return text.replace(/<private>[\s\S]*?<\/private>/gi, "[REDACTED]");
}

export function isFullyPrivate(text: string): boolean {
  const stripped = stripPrivateContent(text)
    .replace(/\[REDACTED\]/g, "")
    .trim();
  return stripped.length === 0;
}
