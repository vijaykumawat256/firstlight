import type { ReactNode } from "react";

/** `**bold**` and `*italic*` only. Everything is plain text: no HTML, no links, nothing executable. */
function inline(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*|\*[^*]+\*)/g).map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**")) return <strong key={i} className="font-semibold text-fg">{part.slice(2, -2)}</strong>;
    if (part.startsWith("*") && part.endsWith("*") && part.length > 2) return <em key={i}>{part.slice(1, -1)}</em>;
    return part;
  });
}

/** Renders the small Markdown subset our own documents use: headings, paragraphs and bullet lists. */
export function Markdown({ source }: { source: string }) {
  const blocks: ReactNode[] = [];
  let list: string[] = [];
  let para: string[] = [];
  const flush = () => {
    if (para.length) blocks.push(<p key={blocks.length} className="text-body text-fg-2">{inline(para.join(" "))}</p>);
    if (list.length)
      blocks.push(
        <ul key={blocks.length} className="flex list-disc flex-col gap-1.5 pl-5 text-body text-fg-2">
          {list.map((l, i) => (
            <li key={i}>{inline(l)}</li>
          ))}
        </ul>,
      );
    para = [];
    list = [];
  };
  for (const raw of source.split("\n")) {
    const line = raw.trimEnd();
    if (line.startsWith("# ")) continue; // the page supplies its own title
    if (line.startsWith("## ")) {
      flush();
      blocks.push(<h3 key={blocks.length} className="mt-2 text-headline font-semibold text-fg">{line.slice(3)}</h3>);
    } else if (line.startsWith("- ")) {
      if (para.length) flush();
      list.push(line.slice(2));
    } else if (line.trim() === "") flush();
    else {
      if (list.length) flush();
      para.push(line.trim());
    }
  }
  flush();
  return <div className="flex select-text flex-col gap-3">{blocks}</div>;
}
