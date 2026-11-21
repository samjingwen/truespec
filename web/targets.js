const SVG = "http://www.w3.org/2000/svg";
const INNER_MARGIN = 9;
const OUTER_MARGIN = 18;

export function targetKey(target) {
  return target ? `${target.kind}:${target.line ?? target.paragraph ?? target.list ?? target.heading}` : "";
}

export function viewportBounds() {
  const view = window.visualViewport;
  const left = view?.offsetLeft ?? 0;
  const top = view?.offsetTop ?? 0;
  return { left, top, right: left + (view?.width ?? innerWidth), bottom: top + (view?.height ?? innerHeight) };
}

export class ReviewTargets {
  constructor(article, outline, toolbar) {
    this.article = article;
    this.outline = outline;
    this.toolbar = toolbar;
    this.reset();
  }

  reset({ lines = [], headings = [] } = {}) {
    this.numbers = lines.map(({ number }) => number);
    this.lines = new Map(lines.map(({ number, parts }) => [number, parts]));
    this.runs = new Map([...this.article.querySelectorAll("[data-run]")].map(span => [Number(span.dataset.run), span.firstChild]));
    this.byRun = new Map();
    for (const { number, parts } of lines) {
      for (const part of parts) {
        if (!this.byRun.has(part.run)) this.byRun.set(part.run, []);
        this.byRun.get(part.run).push({ number, end: part.end });
      }
    }
    this.paragraphs = new Map([...this.article.querySelectorAll("p[data-paragraph]")]
      .filter(paragraph => paragraph.querySelector("[data-run]"))
      .map(paragraph => [Number(paragraph.dataset.paragraph), paragraph]));
    this.paragraphOrder = [...this.paragraphs.values()];
    this.lists = new Map([...this.article.querySelectorAll("[data-list]")]
      .filter(list => list.querySelector("[data-run]"))
      .map(list => [Number(list.dataset.list), list]));
    this.listOrder = [...this.lists.values()];
    const headingNodes = new Map([...this.article.querySelectorAll("h1[id],h2[id],h3[id],h4[id],h5[id],h6[id]")]
      .map(heading => [heading.id, heading]));
    this.sections = headings.filter(heading => headingNodes.has(heading.id))
      .map(heading => ({ ...heading, node: headingNodes.get(heading.id) }));
    const end = new Array(7);
    for (let index = this.sections.length - 1; index >= 0; index--) {
      const section = this.sections[index];
      section.end = end[section.level];
      end.fill(section.node, section.level);
    }
    this.byHeading = new Map(this.sections.map(section => [section.id, section]));
    this.draw(undefined);
  }

  element(target) {
    if (target?.kind === "line") {
      return this.runs.get(this.lines.get(target.line)?.[0]?.run)?.parentElement;
    }
    if (target?.kind === "paragraph") return this.paragraphs.get(target.paragraph);
    if (target?.kind === "list") return this.lists.get(target.list);
    return this.byHeading.get(target?.heading)?.node;
  }

  caption(target) {
    if (target.kind === "line") return `Line ${target.line}`;
    if (target.kind === "paragraph") return "Paragraph";
    if (target.kind === "list") return "List";
    const title = this.byHeading.get(target.heading)?.title;
    return title ? `Section: ${title}` : "Section";
  }

  bounds() {
    const area = viewportBounds();
    const reader = this.article.closest(".reader").getBoundingClientRect();
    const toolbar = this.toolbar.getBoundingClientRect();
    return {
      left: Math.max(area.left, reader.left),
      top: Math.max(area.top, reader.top, toolbar.bottom),
      right: Math.min(area.right, reader.right),
      bottom: Math.min(area.bottom, reader.bottom),
    };
  }

  rectangles(target) {
    if (this.article.hidden || !target) return [];
    const area = this.bounds();
    if (target.kind === "paragraph" || target.kind === "list") {
      return clipOne(this.element(target)?.getBoundingClientRect(), area);
    }
    if (target.kind === "section") {
      const section = this.byHeading.get(target.heading);
      if (!section) return [];
      const range = new Range();
      range.setStartBefore(section.node);
      if (section.end) range.setEndBefore(section.end);
      else range.setEnd(this.article, this.article.childNodes.length);
      const content = range.getBoundingClientRect();
      const article = this.article.getBoundingClientRect();
      return clipOne({
        left: article.left - OUTER_MARGIN + 4,
        right: article.right,
        top: content.top - 4,
        bottom: content.bottom + 4,
      }, area);
    }
    return this.lineRectangles(target.line, area);
  }

  lineRectangles(number, area = this.bounds()) {
    if (!this.lines.has(number)) return [];
    const range = new Range();
    const fragments = [];
    for (const { run, start, end } of this.lines.get(number)) {
      const text = this.runs.get(run);
      if (!text || end > text.length) continue;
      range.setStart(text, start);
      range.setEnd(text, end);
      const scroller = text.parentElement.closest("pre, .table-scroll")?.getBoundingClientRect() ?? area;
      for (const rect of range.getClientRects()) {
        const left = Math.max(rect.left, scroller.left, area.left);
        const top = Math.max(rect.top, scroller.top, area.top);
        const right = Math.min(rect.right, scroller.right, area.right);
        const bottom = Math.min(rect.bottom, scroller.bottom, area.bottom);
        if (right > left && bottom > top) fragments.push({ left, top, right, bottom });
      }
    }
    return mergeRows(fragments);
  }

  lineAt(x, y) {
    const point = document.caretPositionFromPoint?.(x, y);
    const range = point ? null : document.caretRangeFromPoint?.(x, y);
    const text = point?.offsetNode ?? range?.startContainer;
    const span = text?.parentElement?.closest("[data-run]");
    if (!span || !this.article.contains(span)) return undefined;
    const parts = this.byRun.get(Number(span.dataset.run));
    if (!parts?.length) return undefined;
    const offset = point?.offset ?? range.startOffset;
    let left = 0;
    let right = parts.length;
    while (left < right) {
      const middle = (left + right) >>> 1;
      if (parts[middle].end < offset) left = middle + 1;
      else right = middle;
    }
    for (const part of [parts[left], parts[left - 1], parts[left + 1]]) {
      if (part && this.lineRectangles(part.number).some(rect =>
        x >= rect.left - 2 && x <= rect.right + 2 && y >= rect.top && y <= rect.bottom)) {
        return part.number;
      }
    }
    return undefined;
  }

  sectionAt(y) {
    const heading = precedingAtY(this.sections, y, section => section.node)?.id;
    return heading ? { kind: "section", heading } : undefined;
  }

  paragraphFor(element) {
    const paragraph = Number(element?.closest("p[data-paragraph]")?.dataset.paragraph);
    return this.paragraphs.has(paragraph) ? { kind: "paragraph", paragraph } : undefined;
  }

  listFor(element) {
    const list = Number(element?.closest("[data-list]")?.dataset.list);
    return this.lists.has(list) ? { kind: "list", list } : undefined;
  }

  at(x, y, element) {
    if (this.article.hidden) return undefined;
    const article = this.article.getBoundingClientRect();
    const area = this.bounds();
    if (y < Math.max(area.top, article.top) || y >= Math.min(area.bottom, article.bottom) || x >= article.right) return undefined;
    if (x >= article.left - OUTER_MARGIN && x < article.left - INNER_MARGIN) {
      return this.sectionAt(y);
    }
    if (x >= article.left - INNER_MARGIN && x < article.left) {
      return this.listFor(blockAtY(this.listOrder, y)) ?? this.paragraphFor(blockAtY(this.paragraphOrder, y));
    }
    if (x < article.left) return undefined;
    const line = this.lineAt(x, y);
    return line === undefined ? this.paragraphFor(element) : { kind: "line", line };
  }

  scopesForLine(line) {
    const target = { kind: "line", line };
    const element = this.element(target);
    if (!element) return [];
    return [target, this.paragraphFor(element), this.listFor(element),
      this.sectionAt(element.getBoundingClientRect().top + 1)].filter(Boolean);
  }

  draw(target, active = false) {
    const boxes = this.rectangles(target);
    this.outline.toggleAttribute("hidden", boxes.length === 0);
    this.outline.classList.toggle("active", active);
    for (const name of ["kind", "line", "paragraph", "list", "heading"]) {
      if (boxes.length && target[name] !== undefined) this.outline.dataset[name] = target[name];
      else delete this.outline.dataset[name];
    }
    const shapes = boxes.map(box => svg("rect", {
      class: "target-box", x: box.left - 3, y: box.top - 1,
      width: box.right - box.left + 6, height: box.bottom - box.top + 2, rx: 3,
    }));
    this.outline.replaceChildren(...shapes);
    return boxes;
  }
}

function svg(tag, attributes) {
  const element = document.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attributes)) element.setAttribute(name, value);
  return element;
}

function clipOne(rect, area) {
  if (!rect) return [];
  const left = Math.max(rect.left, area.left);
  const right = Math.min(rect.right, area.right);
  const top = Math.max(rect.top, area.top);
  const bottom = Math.min(rect.bottom, area.bottom);
  return right > left && bottom > top ? [{ left, right, top, bottom }] : [];
}

function precedingAtY(items, y, element) {
  let left = 0;
  let right = items.length;
  while (left < right) {
    const middle = (left + right) >>> 1;
    if (element(items[middle]).getBoundingClientRect().top <= y) left = middle + 1;
    else right = middle;
  }
  return items[left - 1];
}

function blockAtY(items, y) {
  const block = precedingAtY(items, y, element => element);
  return block?.getBoundingClientRect().bottom > y ? block : undefined;
}

function mergeRows(fragments) {
  const rows = [];
  for (const rect of fragments.sort((a, b) => a.top - b.top || a.left - b.left)) {
    const row = rows.at(-1);
    const overlap = row ? Math.min(row.bottom, rect.bottom) - Math.max(row.top, rect.top) : 0;
    if (row && overlap > Math.min(row.bottom - row.top, rect.bottom - rect.top) / 2) {
      row.left = Math.min(row.left, rect.left);
      row.right = Math.max(row.right, rect.right);
      row.top = Math.min(row.top, rect.top);
      row.bottom = Math.max(row.bottom, rect.bottom);
    } else {
      rows.push(rect);
    }
  }
  return rows;
}
