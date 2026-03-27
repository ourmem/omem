const KEYWORDS: readonly string[] = [
  "remember",
  "save this",
  "don't forget",
  "keep in mind",
  "note that",
  "store this",
  "memorize",
  "记住",
  "记一下",
  "保存",
  "记下来",
  "别忘了",
] as const;

export function detectKeyword(text: string): boolean {
  const lower = text.toLowerCase();
  return KEYWORDS.some((kw) => lower.includes(kw));
}

export const KEYWORD_NUDGE =
  "The user appears to want you to remember something. " +
  "Consider using the `memory_store` tool to save this information for future reference.";
