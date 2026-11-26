/** Finds http(s) links in terminal text. A terminal wraps a long link (the Claude sign-in URL is
 *  ~300 characters) over several screen rows, so the caller joins the rows of one wrapped line and
 *  this finds the links in the joined text; `start`/`end` are offsets into it. */
export interface TermLink { url: string; start: number; end: number }

const URL_RE = /https?:\/\/[^\s<>"'`\x00-\x1f]+/g;

export function findLinks(text: string): TermLink[] {
  const out: TermLink[] = [];
  for (const m of text.matchAll(URL_RE)) {
    // Punctuation that ends a sentence or closes a bracket around the link is not part of it.
    let url = m[0];
    while (/[.,;:!?)\]}]$/.test(url) && !(url.endsWith(")") && url.includes("("))) url = url.slice(0, -1);
    if (url.length > 8) out.push({ url, start: m.index!, end: m.index! + url.length });
  }
  return out;
}
