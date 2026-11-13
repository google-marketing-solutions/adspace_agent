// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Lightweight Markdown → DOM rendering for transcript text (pure DOM builders, no app state).

const appendPlainTextWithArtifacts = (parent, textSlice, options = {}) => {
  if (!textSlice) return;
  const resolve = options?.resolveArtifactUrl;
  if (typeof resolve !== "function") {
    parent.appendChild(document.createTextNode(textSlice));
    return;
  }
  const fileRe = /\b([A-Za-z0-9_][A-Za-z0-9_.-]*\.[A-Za-z0-9]{2,8})\b/g;
  let last = 0;
  let m;
  while ((m = fileRe.exec(textSlice)) !== null) {
    const candidate = m[1];
    const href = resolve(candidate);
    if (!href) continue;
    if (m.index > last) {
      parent.appendChild(document.createTextNode(textSlice.slice(last, m.index)));
    }
    const a = document.createElement("a");
    a.className = "artifact-inline-link";
    a.href = href;
    a.target = "_blank";
    a.rel = "noreferrer";
    a.textContent = candidate;
    parent.appendChild(a);
    last = fileRe.lastIndex;
  }
  if (last < textSlice.length) {
    parent.appendChild(document.createTextNode(textSlice.slice(last)));
  }
};

export const appendInlineWithCodeTable = (parent, text, codes, options = {}) => {
  if (!text) return;
  // NUL is used as a sentinel around masked inline-code tokens (see
  // `appendInlineMarkdown`); it cannot appear in real message text.
  const tokenRegex =
    // eslint-disable-next-line no-control-regex
    /(\u0000CODE(\d+)\u0000|\[([^\]]+)\]\((https?:\/\/[^\s)]+|mailto:[^\s)]+|\/(?!\/)[^\s)]*|[A-Za-z0-9_][A-Za-z0-9_.-]*\.[A-Za-z0-9]{2,8})\)|\*\*\*([\s\S]+?)\*\*\*|\*\*([\s\S]+?)\*\*|__(?=\S)([\s\S]*?\S)__|~~([\s\S]+?)~~|(?<!\*)\*(?=\S)([\s\S]*?\S)\*(?!\*)|(?<![a-zA-Z0-9_])_(?=\S)([\s\S]*?\S)_(?![a-zA-Z0-9_]))/g;

  let lastIdx = 0;
  let match;
  while ((match = tokenRegex.exec(text)) !== null) {
    if (match.index > lastIdx) {
      appendPlainTextWithArtifacts(parent, text.slice(lastIdx, match.index), options);
    }
    if (match[2] !== undefined) {
      const codeText = codes[Number(match[2])] ?? "";
      const code = document.createElement("code");
      code.textContent = codeText;
      const artifactHref =
        typeof options?.resolveArtifactUrl === "function"
          ? options.resolveArtifactUrl(codeText.trim())
          : null;
      if (artifactHref) {
        const a = document.createElement("a");
        a.className = "artifact-inline-link";
        a.href = artifactHref;
        a.target = "_blank";
        a.rel = "noreferrer";
        a.appendChild(code);
        parent.appendChild(a);
      } else {
        parent.appendChild(code);
      }
    } else if (match[3] !== undefined && match[4] !== undefined) {
      const rawHref = match[4];
      const safeHref = /^(?:https?:\/\/|mailto:|\/(?!\/))/i.test(rawHref)
        ? rawHref
        : typeof options?.resolveArtifactUrl === "function"
          ? options.resolveArtifactUrl(rawHref)
          : null;
      if (safeHref) {
        const a = document.createElement("a");
        appendInlineWithCodeTable(a, match[3], codes, options);
        a.href = safeHref;
        a.target = "_blank";
        a.rel = "noreferrer";
        parent.appendChild(a);
      } else {
        parent.appendChild(document.createTextNode("["));
        appendInlineWithCodeTable(parent, match[3], codes, options);
        parent.appendChild(document.createTextNode(`](${rawHref})`));
      }
    } else if (match[5] !== undefined) {
      const strong = document.createElement("strong");
      const em = document.createElement("em");
      appendInlineWithCodeTable(em, match[5], codes, options);
      strong.appendChild(em);
      parent.appendChild(strong);
    } else if (match[6] !== undefined || match[7] !== undefined) {
      const strong = document.createElement("strong");
      appendInlineWithCodeTable(strong, match[6] ?? match[7], codes, options);
      parent.appendChild(strong);
    } else if (match[8] !== undefined) {
      const del = document.createElement("del");
      appendInlineWithCodeTable(del, match[8], codes, options);
      parent.appendChild(del);
    } else if (match[9] !== undefined || match[10] !== undefined) {
      const em = document.createElement("em");
      appendInlineWithCodeTable(em, match[9] ?? match[10], codes, options);
      parent.appendChild(em);
    }
    lastIdx = tokenRegex.lastIndex;
  }
  if (lastIdx < text.length) {
    appendPlainTextWithArtifacts(parent, text.slice(lastIdx), options);
  }
};

export const appendInlineMarkdown = (parent, rawText, options = {}) => {
  const str = String(rawText ?? "");
  if (!str) return;
  const codes = [];
  const masked = str.replace(/`([^`\n]+)`/g, (_, codeContent) => {
    const idx = codes.length;
    codes.push(codeContent);
    return `\u0000CODE${idx}\u0000`;
  });
  appendInlineWithCodeTable(parent, masked, codes, options);
};

export const isHrLine = (line) => /^\s*([-*_])(?:\s*\1){2,}\s*$/.test(line);
export const isHeadingLine = (line) => /^\s*#{1,6}\s+\S/.test(line);
export const isFenceLine = (line) => /^\s*(`{3,}|~{3,})/.test(line);
export const isBlockquoteLine = (line) => /^\s*>\s?/.test(line);
export const isListLine = (line) => !isHrLine(line) && /^\s*(?:[-*+]|\d+[.)])\s+/.test(line);
export const isTableSepLine = (line) =>
  /^\s*\|?(?:\s*:?-{2,}:?\s*\|)+\s*:?-{2,}:?\s*\|?\s*$/.test(line);
export const isTableStart = (lines, idx) =>
  idx + 1 < lines.length &&
  lines[idx].includes("|") &&
  isTableSepLine(lines[idx + 1]);

export const splitTableCells = (line) => {
  let trimmed = line.trim();
  if (trimmed.startsWith("|")) trimmed = trimmed.slice(1);
  if (trimmed.endsWith("|")) trimmed = trimmed.slice(0, -1);
  return trimmed.split("|").map((c) => c.trim());
};

export const parseMarkdownBlocksInto = (container, text, options = {}) => {
  const lines = String(text || "")
    .replace(/\r\n/g, "\n")
    .split("\n");

  const parseList = (startIdx, baseIndent, parentEl) => {
    let i = startIdx;
    const firstMatch = lines[i].match(/^(\s*)([-*+]|\d+[.)])\s+(.*)$/);
    if (!firstMatch) return i + 1;

    const isOrdered = /^\d+[.)]$/.test(firstMatch[2]);
    const listEl = document.createElement(isOrdered ? "ol" : "ul");
    if (isOrdered) {
      const startNum = parseInt(firstMatch[2], 10);
      if (Number.isFinite(startNum) && startNum !== 1) {
        listEl.start = startNum;
      }
    }
    let lastLi = null;

    while (i < lines.length) {
      const line = lines[i];
      if (line.trim() === "") {
        let nextNonEmpty = i + 1;
        while (nextNonEmpty < lines.length && lines[nextNonEmpty].trim() === "") {
          nextNonEmpty++;
        }
        if (nextNonEmpty < lines.length && isListLine(lines[nextNonEmpty])) {
          const nextIndent = lines[nextNonEmpty]
            .match(/^(\s*)/)[1]
            .replace(/\t/g, "  ").length;
          if (nextIndent >= baseIndent) {
            i = nextNonEmpty;
            continue;
          }
        }
        break;
      }

      if (isHrLine(line) || isFenceLine(line) || isHeadingLine(line) || isBlockquoteLine(line)) {
        break;
      }

      const itemMatch = line.match(/^(\s*)([-*+]|\d+[.)])\s+(.*)$/);
      if (itemMatch) {
        const indent = itemMatch[1].replace(/\t/g, "  ").length;
        if (indent < baseIndent) {
          break;
        }
        if (indent > baseIndent) {
          i = parseList(i, indent, lastLi || listEl);
          continue;
        }
        const itemOrdered = /^\d+[.)]$/.test(itemMatch[2]);
        if (itemOrdered !== isOrdered) {
          break;
        }
        const li = document.createElement("li");
        appendInlineMarkdown(li, itemMatch[3], options);
        listEl.appendChild(li);
        lastLi = li;
        i++;
        continue;
      }

      const contIndent = line.match(/^(\s*)/)[1].replace(/\t/g, "  ").length;
      if (lastLi && contIndent > baseIndent) {
        lastLi.appendChild(document.createElement("br"));
        appendInlineMarkdown(lastLi, line.trim(), options);
        i++;
        continue;
      }
      break;
    }

    parentEl.appendChild(listEl);
    return i;
  };

  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    if (line.trim() === "") {
      i++;
      continue;
    }

    // Fenced code block
    const fenceMatch = line.match(/^\s*(`{3,}|~{3,})(.*)$/);
    if (fenceMatch) {
      const fence = fenceMatch[1];
      const codeLines = [];
      i++;
      const closeRe = new RegExp(`^\\s*${fence[0]}{${fence.length},}\\s*$`);
      while (i < lines.length && !closeRe.test(lines[i])) {
        codeLines.push(lines[i]);
        i++;
      }
      if (i < lines.length) i++; // skip closing fence
      const pre = document.createElement("pre");
      const code = document.createElement("code");
      code.textContent = codeLines.join("\n");
      pre.appendChild(code);
      container.appendChild(pre);
      continue;
    }

    // Horizontal rule
    if (isHrLine(line)) {
      container.appendChild(document.createElement("hr"));
      i++;
      continue;
    }

    // ATX Heading (# .. ######)
    const headingMatch = line.match(/^\s*(#{1,6})\s+(.+?)\s*#*$/);
    if (headingMatch) {
      const level = Math.min(4, headingMatch[1].length);
      const h = document.createElement(`h${level}`);
      appendInlineMarkdown(h, headingMatch[2], options);
      container.appendChild(h);
      i++;
      continue;
    }

    // Blockquote
    if (isBlockquoteLine(line)) {
      const quoteLines = [];
      while (i < lines.length && isBlockquoteLine(lines[i])) {
        quoteLines.push(lines[i].replace(/^\s*>\s?/, ""));
        i++;
      }
      const bq = document.createElement("blockquote");
      parseMarkdownBlocksInto(bq, quoteLines.join("\n"), options);
      container.appendChild(bq);
      continue;
    }

    // GFM Table
    if (isTableStart(lines, i)) {
      const headers = splitTableCells(lines[i]);
      i += 2; // skip header + separator
      const table = document.createElement("table");
      const thead = document.createElement("thead");
      const trHead = document.createElement("tr");
      for (const hCell of headers) {
        const th = document.createElement("th");
        appendInlineMarkdown(th, hCell, options);
        trHead.appendChild(th);
      }
      thead.appendChild(trHead);
      const tbody = document.createElement("tbody");
      while (i < lines.length && lines[i].trim() !== "" && lines[i].includes("|")) {
        const cells = splitTableCells(lines[i]);
        const tr = document.createElement("tr");
        headers.forEach((_, colIdx) => {
          const td = document.createElement("td");
          appendInlineMarkdown(td, cells[colIdx] ?? "", options);
          tr.appendChild(td);
        });
        tbody.appendChild(tr);
        i++;
      }
      table.append(thead, tbody);
      container.appendChild(table);
      continue;
    }

    // Unordered or Ordered List
    if (isListLine(line)) {
      const baseIndent = line.match(/^(\s*)/)[1].replace(/\t/g, "  ").length;
      i = parseList(i, baseIndent, container);
      continue;
    }

    // Paragraph
    const paraLines = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !isFenceLine(lines[i]) &&
      !isHrLine(lines[i]) &&
      !isHeadingLine(lines[i]) &&
      !isBlockquoteLine(lines[i]) &&
      !isListLine(lines[i]) &&
      !isTableStart(lines, i)
    ) {
      paraLines.push(lines[i]);
      i++;
    }
    if (paraLines.length > 0) {
      const p = document.createElement("p");
      paraLines.forEach((pLine, idx) => {
        if (idx > 0) p.appendChild(document.createElement("br"));
        appendInlineMarkdown(p, pLine, options);
      });
      container.appendChild(p);
    }
  }
};
