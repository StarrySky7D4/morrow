export class MarkdownLine {
  text: string = '';
  kind: string = 'text';
  level: number = 0;
}
// Native, offline preview: headings, lists, quotes, fences and paragraphs.
// Unsupported inline syntax remains visible; no HTML, remote images or links execute.
export function markdownLines(value: string): MarkdownLine[] {
  let code = false;
  const result: MarkdownLine[] = [];
  value.split('\n').forEach((text: string) => {
    if (/^\s*```/.test(text)) { code = !code; return; }
    const line = new MarkdownLine(); line.text = text;
    if (code) { line.kind = 'code'; }
    else if (/^#{1,6}\s/.test(text)) { line.kind = 'heading'; line.level = text.indexOf(' '); line.text = text.substring(line.level + 1); }
    else if (/^\s*>\s?/.test(text)) { line.kind = 'quote'; line.text = text.replace(/^\s*>\s?/, '│ '); }
    else if (/^\s*[-*+]\s/.test(text)) { line.kind = 'list'; line.text = text.replace(/^\s*[-*+]\s/, '• '); }
    result.push(line);
  });
  return result;
}
