// Minimal Markdown for chat replies: headings, bold, italics, inline code, code
// blocks, lists, quotes and rules. Builds DOM nodes directly (never innerHTML),
// so whatever a model writes can't inject markup into the island.

import { h } from "./dom";

export function renderMarkdown(text: string): DocumentFragment {
  const out = document.createDocumentFragment();
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let list: HTMLElement | null = null;
  let para: string[] = [];

  const flushPara = () => {
    if (para.length) {
      const p = h("p", { class: "md-p" });
      para.forEach((line, i) => {
        if (i) p.append(document.createElement("br"));
        p.append(inline(line));
      });
      out.append(p);
      para = [];
    }
  };
  const flushList = () => {
    if (list) out.append(list);
    list = null;
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Fenced code block. An unclosed fence (mid-stream) runs to the end.
    if (/^\s*```/.test(line)) {
      flushPara();
      flushList();
      const code: string[] = [];
      while (++i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i]);
      out.append(h("pre", { class: "md-pre" }, h("code", { text: code.join("\n") })));
      continue;
    }

    if (!line.trim()) {
      flushPara();
      flushList();
      continue;
    }

    const heading = /^\s*(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      flushPara();
      flushList();
      out.append(h("div", { class: `md-h md-h${Math.min(heading[1].length, 3)}` }, inline(heading[2])));
      continue;
    }

    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flushPara();
      flushList();
      out.append(h("hr", { class: "md-hr" }));
      continue;
    }

    const quote = /^\s*>\s?(.*)$/.exec(line);
    if (quote) {
      flushPara();
      flushList();
      out.append(h("div", { class: "md-quote" }, inline(quote[1])));
      continue;
    }

    const bullet = /^\s*[-*+]\s+(.*)$/.exec(line);
    const numbered = /^\s*(\d+)[.)]\s+(.*)$/.exec(line);
    if (bullet || numbered) {
      flushPara();
      const tag = numbered ? "ol" : "ul";
      if (!list || list.tagName.toLowerCase() !== tag) {
        flushList();
        list = h(tag, { class: "md-list" });
        if (numbered && numbered[1] !== "1") list.setAttribute("start", numbered[1]);
      }
      list.append(h("li", {}, inline(numbered ? numbered[2] : bullet![1])));
      continue;
    }

    flushList();
    para.push(line);
  }
  flushPara();
  flushList();
  return out;
}

/** **bold**, __bold__, *italic*, _italic_, `code`, ~~strike~~. Unclosed markers stay as text. */
function inline(text: string): DocumentFragment {
  const frag = document.createDocumentFragment();
  const re = /`([^`]+)`|\*\*(.+?)\*\*|__(.+?)__|~~(.+?)~~|\*([^*\s][^*]*?)\*|(?<![\w])_([^_\s][^_]*?)_(?![\w])/g;
  let last = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (m.index > last) frag.append(text.slice(last, m.index));
    if (m[1] != null) frag.append(h("code", { class: "md-code", text: m[1] }));
    else if (m[2] != null || m[3] != null) frag.append(h("strong", {}, inline(m[2] ?? m[3])));
    else if (m[4] != null) frag.append(h("s", {}, inline(m[4])));
    else frag.append(h("em", {}, inline(m[5] ?? m[6])));
    last = re.lastIndex;
  }
  if (last < text.length) frag.append(text.slice(last));
  return frag;
}
