import { openUrl } from "@tauri-apps/plugin-opener";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

const WEB = /^(https?:|mailto:)/i;
const LINK = "text-left text-graphite underline decoration-mist underline-offset-4 hover:text-carbon-ink";

// Transcript text is untrusted (it quotes web pages and tool output): raw HTML is dropped,
// links open in the browser only on click, and images become text (CSP blocks remote ones anyway).
const components: Components = {
  // Relative links (`src/file.ts#L4`) point into a checkout the viewer cannot open: plain text.
  a: ({ href, children }) =>
    href && WEB.test(href) ? (
      <button type="button" title={href} onClick={() => openUrl(href).catch((e) => console.warn("open link", e))} className={LINK}>
        {children}
      </button>
    ) : (
      <span title={href}>{children}</span>
    ),
  img: ({ alt, src }) => <span className="text-ashen">[{alt || String(src ?? "")}]</span>,
  h1: ({ children }) => <h3 className="font-serif text-[22px] leading-snug">{children}</h3>,
  h2: ({ children }) => <h4 className="font-serif text-[19px] leading-snug">{children}</h4>,
  h3: ({ children }) => <h5 className="text-[16px] font-medium">{children}</h5>,
  p: ({ children }) => <p className="leading-relaxed">{children}</p>,
  ul: ({ children }) => <ul className="flex list-disc flex-col gap-1 pl-6">{children}</ul>,
  ol: ({ children }) => <ol className="flex list-decimal flex-col gap-1 pl-6">{children}</ol>,
  blockquote: ({ children }) => <blockquote className="border-l-2 border-chalk pl-4 text-graphite">{children}</blockquote>,
  pre: ({ children }) => <pre className="overflow-x-auto rounded-control bg-soft-stone p-3 font-mono text-caption leading-relaxed [&>code]:bg-transparent [&>code]:p-0">{children}</pre>,
  code: ({ children }) => <code className="rounded bg-soft-stone px-1 font-mono text-[0.88em]">{children}</code>,
  table: ({ children }) => (
    <div className="overflow-x-auto">
      <table className="border-collapse text-caption">{children}</table>
    </div>
  ),
  th: ({ children }) => <th className="border border-chalk px-2 py-1 text-left font-medium">{children}</th>,
  td: ({ children }) => <td className="border border-chalk px-2 py-1 align-top">{children}</td>,
  hr: () => <hr className="border-chalk" />,
};

export function MarkdownText({ text }: { text: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-3 break-words text-[15px] text-carbon-ink">
      <Markdown remarkPlugins={[remarkGfm]} components={components} skipHtml>
        {text}
      </Markdown>
    </div>
  );
}
