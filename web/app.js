import { ReviewTargets, targetKey, viewportBounds } from "./targets.js";

const elements = Object.fromEntries([...document.querySelectorAll("[id]")].map(node => [node.id, node]));
const {
  article, reader, documents, composer, comment, confirmation,
} = elements;
const lineComposer = elements["line-composer"];
const lineComment = elements["line-comment"];
const generalComposer = elements["general-composer"];
const generalComposerBody = elements["general-composer-body"];
const generalComposerToggle = elements["general-composer-toggle"];
const generalComment = elements["general-comment"];
const workspaceMatch = location.pathname.match(/^\/w\/([^/]+)\/?$/);
const workspaceId = workspaceMatch ? decodeURIComponent(workspaceMatch[1]) : undefined;
const wideLayout = window.matchMedia("(min-width: 60rem)");
const PANEL_KEYS = {
  documents: "truespec.sidebar.documents",
  review: "truespec.sidebar.review",
};
const DOCUMENT_DISCLOSURE_PREFIX = "truespec.documents.disclosure:";
const REVIEW_WIDTH_KEY = "truespec.sidebar.review.width";
const GENERAL_COMPOSER_COLLAPSED_KEY = "truespec.general-composer.collapsed";
const REVIEW_MIN_REM = 17;
const REVIEW_MAX_REM = 38;
const READER_MIN_REM = 22;
const COMPOSER_GAP = 8;
const COMPOSER_MARGIN = 10;
const targets = new ReviewTargets(article, elements["line-outline"], elements["doc-toolbar"]);
const cache = new Map();
let entries = [];
let reviewId;
let board;
let currentId;
let page;
let documentRequest = 0;
let selection;
let composing;
let anchoredDraft;
let hoverTarget;
let keyboardLine;
let keyboardScope = 0;
let markedTarget;
let outlineFrame = 0;
let selectionFrame = 0;
let pointerStart;
let busy = false;
let refreshing = false;
let lastSubmit;
let annotationEnabled = true;
let documentsOpen = true;
let reviewOpen = true;
let reviewExpanded = false;
let reviewWidthPreference = storedReviewWidth();
let reviewDrag;
let generalComposerCollapsed = storedGeneralComposerCollapsed();

class RequestError extends Error {
  constructor(message, status, code) {
    super(message);
    this.status = status;
    this.confirmation = code;
  }
}

function storedPanel(name, fallback = true) {
  try {
    const value = localStorage.getItem(PANEL_KEYS[name]);
    return value === null ? fallback : value === "true";
  } catch {
    return fallback;
  }
}

function savePanel(name, value) {
  try { localStorage.setItem(PANEL_KEYS[name], String(value)); }
  catch { /* A private browser may refuse persistence. */ }
}

function storedDisclosure(key, fallback) {
  try {
    const value = localStorage.getItem(`${DOCUMENT_DISCLOSURE_PREFIX}${key}`);
    return value === null ? fallback : value === "true";
  } catch {
    return fallback;
  }
}

function saveDisclosure(key, value) {
  try { localStorage.setItem(`${DOCUMENT_DISCLOSURE_PREFIX}${key}`, String(value)); }
  catch { /* A private browser may refuse persistence. */ }
}

function disclosure(className, key, label, defaultOpen) {
  const details = node("details", className);
  details.dataset.disclosureKey = key;
  details.open = storedDisclosure(key, defaultOpen);
  const summary = node("summary", "");
  summary.dataset.disclosureKey = key;
  summary.append(
    node("span", "disclosure-chevron"),
    node("span", "disclosure-label", label),
  );
  details.append(summary);
  details.addEventListener("toggle", () => saveDisclosure(key, details.open));
  return details;
}

function storedReviewWidth() {
  try {
    const value = Number(localStorage.getItem(REVIEW_WIDTH_KEY));
    return Number.isFinite(value) && value > 0 ? value : undefined;
  } catch {
    return undefined;
  }
}

function saveReviewWidth(value) {
  try {
    if (value === undefined) localStorage.removeItem(REVIEW_WIDTH_KEY);
    else localStorage.setItem(REVIEW_WIDTH_KEY, String(Math.round(value)));
  } catch {
    /* A private browser may refuse persistence. */
  }
}

function storedGeneralComposerCollapsed() {
  try { return localStorage.getItem(GENERAL_COMPOSER_COLLAPSED_KEY) === "true"; }
  catch { return false; }
}

function saveGeneralComposerCollapsed(value) {
  try { localStorage.setItem(GENERAL_COMPOSER_COLLAPSED_KEY, String(value)); }
  catch { /* A private browser may refuse persistence. */ }
}

function updateGeneralComposerDisclosure() {
  const pending = !!generalComment.value.trim();
  generalComposer.dataset.collapsed = String(generalComposerCollapsed);
  generalComposer.dataset.pending = String(pending);
  generalComposerBody.hidden = generalComposerCollapsed;
  generalComposerToggle.setAttribute("aria-expanded", String(!generalComposerCollapsed));
  const action = generalComposerCollapsed ? "Expand general feedback" : "Collapse general feedback";
  const label = generalComposerCollapsed && pending ? `${action}, pending feedback` : action;
  generalComposerToggle.setAttribute("aria-label", label);
  generalComposerToggle.title = label;
}

function toggleGeneralComposer() {
  generalComposerCollapsed = !generalComposerCollapsed;
  saveGeneralComposerCollapsed(generalComposerCollapsed);
  updateGeneralComposerDisclosure();
  if (!generalComposerCollapsed) generalComment.focus();
}

function reviewWidthBounds() {
  const rem = Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 15;
  const minimum = Math.round(REVIEW_MIN_REM * rem);
  const documentsWidth = wideLayout.matches && documentsOpen && !reviewExpanded
    ? elements["documents-panel"].getBoundingClientRect().width
    : 0;
  const maximum = Math.max(minimum, Math.floor(Math.min(
    REVIEW_MAX_REM * rem,
    innerWidth * 0.55,
    innerWidth - documentsWidth - READER_MIN_REM * rem,
  )));
  return { minimum, maximum };
}

function clampReviewWidth(value) {
  const { minimum, maximum } = reviewWidthBounds();
  return Math.max(minimum, Math.min(value, maximum));
}

function reviewResizeEnabled() {
  return wideLayout.matches && reviewOpen && !reviewExpanded;
}

function applyReviewWidth() {
  const layout = elements.layout;
  if (reviewWidthPreference === undefined) layout.style.removeProperty("--review-width");
  else layout.style.setProperty("--review-width", `${clampReviewWidth(reviewWidthPreference)}px`);
  const { minimum, maximum } = reviewWidthBounds();
  const current = reviewWidthPreference === undefined
    ? elements.review.getBoundingClientRect().width
    : clampReviewWidth(reviewWidthPreference);
  const resizer = elements["review-resizer"];
  const enabled = reviewResizeEnabled();
  resizer.hidden = !enabled;
  resizer.tabIndex = enabled ? 0 : -1;
  resizer.setAttribute("aria-disabled", String(!enabled));
  resizer.setAttribute("aria-valuemin", String(Math.round(minimum)));
  resizer.setAttribute("aria-valuemax", String(Math.round(maximum)));
  resizer.setAttribute("aria-valuenow", String(Math.round(current)));
  resizer.setAttribute("aria-valuetext", `${Math.round(current)} pixels`);
}

function applyPanels(focusPanel) {
  const layout = elements.layout;
  layout.dataset.documentsOpen = String(documentsOpen);
  layout.dataset.reviewOpen = String(reviewOpen);
  layout.dataset.reviewExpanded = String(reviewExpanded);
  elements["documents-toggle"].setAttribute("aria-expanded", String(documentsOpen));
  elements["review-toggle"].setAttribute("aria-expanded", String(reviewOpen));
  elements["review-expand"].setAttribute("aria-pressed", String(reviewExpanded));
  elements["review-expand"].setAttribute("aria-label", reviewExpanded ? "Restore Review" : "Expand Review");
  elements["review-expand"].title = reviewExpanded ? "Restore Review" : "Expand Review";
  elements["documents-panel"].inert = !documentsOpen || reviewExpanded;
  elements.review.inert = !reviewOpen && !reviewExpanded;
  const overlay = !wideLayout.matches && (documentsOpen || reviewOpen);
  reader.inert = overlay || reviewExpanded;
  elements["sidebar-backdrop"].hidden = !overlay || reviewExpanded;
  applyReviewWidth();
  if (focusPanel) {
    focusPanelControl(focusPanel === "documents" ? "documents-close" : "review-close");
  }
  if (reviewOpen && !document.hidden) markRepliesSeen();
  scheduleLineOutline();
  scheduleSelectionAction();
}

function focusPanelControl(id, attempts = 8) {
  setTimeout(() => {
    const control = elements[id];
    control.focus({ preventScroll: true });
    if (document.activeElement !== control && attempts > 1) {
      focusPanelControl(id, attempts - 1);
    }
  }, 25);
}

function setPanel(name, open, { focus = false, remember = true } = {}) {
  if (name === "review" && !open && reviewExpanded) reviewExpanded = false;
  if (!wideLayout.matches && open) {
    documentsOpen = name === "documents";
    reviewOpen = name === "review";
  } else if (name === "documents") {
    documentsOpen = open;
  } else {
    reviewOpen = open;
  }
  if (remember && wideLayout.matches) savePanel(name, open);
  applyPanels(focus && open ? name : undefined);
}

function setReviewExpanded(expanded, { focus = true } = {}) {
  if (!reviewOpen || reviewExpanded === expanded) return;
  if (reviewDrag) finishReviewResize();
  reviewExpanded = expanded;
  applyPanels();
  if (focus) requestAnimationFrame(() => elements["review-expand"].focus());
}

function resizeReviewFromPointer(event) {
  reviewWidthPreference = clampReviewWidth(innerWidth - event.clientX);
  applyReviewWidth();
}

function finishReviewResize(event) {
  if (!reviewDrag || (event && event.pointerId !== reviewDrag.pointerId)) return;
  reviewDrag = undefined;
  elements.layout.dataset.reviewResizing = "false";
  saveReviewWidth(reviewWidthPreference);
}

function resetPanelsForViewport() {
  if (wideLayout.matches) {
    documentsOpen = storedPanel("documents");
    reviewOpen = storedPanel("review");
  } else {
    documentsOpen = false;
    reviewOpen = false;
  }
  applyPanels();
}

function closeOverlay(restoreFocus = false) {
  if (wideLayout.matches) return;
  const toggle = documentsOpen ? elements["documents-toggle"] : elements["review-toggle"];
  documentsOpen = reviewOpen = false;
  applyPanels();
  if (restoreFocus) toggle.focus();
}

async function request(path, data) {
  const options = { cache: "no-store", headers: {} };
  const previous = data === undefined ? cache.get(path) : null;
  if (previous) options.headers["If-None-Match"] = previous.etag;
  if (data !== undefined) {
    options.method = "POST";
    options.headers["Content-Type"] = "application/json";
    options.body = JSON.stringify(data);
  }
  const response = await fetch(path, options);
  if (response.status === 304 && previous) return previous.value;
  if (response.status === 204) return null;
  const isJson = response.headers.get("Content-Type")?.includes("application/json");
  const value = isJson ? await response.json() : await response.text();
  if (!response.ok) {
    throw new RequestError(value?.message || String(value) || "The request could not be completed.", response.status, value?.confirmation);
  }
  const etag = response.headers.get("ETag");
  if (etag) cache.set(path, { etag, value });
  return value;
}

function node(tag, cls, text) {
  const item = document.createElement(tag);
  if (cls) item.className = cls;
  if (text !== undefined) item.textContent = text;
  return item;
}

function makeButton(text, style, action, key) {
  const button = node("button", style, text);
  button.type = "button";
  if (key) button.dataset.focusKey = key;
  button.addEventListener("click", action);
  return button;
}

function textState(key, text = "") {
  elements[key].textContent = text;
  elements[key].hidden = !text;
}

function problem(error) {
  textState("error", error instanceof TypeError
    ? "Cannot connect to TrueSpec. Keep this tab open and check the serve terminal."
    : error.message);
}

function locationUrl(id, fragment) {
  const url = new URL(window.location.href);
  url.search = "";
  url.hash = "";
  if (reviewId) url.searchParams.set("review", reviewId);
  if (id) url.searchParams.set("doc", id);
  if (fragment) url.hash = encodeURIComponent(fragment);
  return url;
}

function currentFragment() {
  try { return decodeURIComponent(window.location.hash.slice(1)); }
  catch { return ""; }
}

function workspacePath() {
  const base = workspaceId
    ? `/api/workspaces/${encodeURIComponent(workspaceId)}`
    : "/api";
  return `${base}/reviews/${encodeURIComponent(reviewId)}`;
}

function workspaceApi(path) {
  return workspaceId
    ? `/api/workspaces/${encodeURIComponent(workspaceId)}/${path}`
    : `/api/${path}`;
}

function renderNav() {
  elements["document-count"].textContent = String(entries.length);
  const list = document.createDocumentFragment();
  const focusedDisclosure = document.activeElement?.dataset.disclosureKey;
  const tree = navigationTree(entries);
  if (!tree.length) list.append(node("p", "muted", "No Markdown documents found."));
  for (const branch of tree) list.append(renderNavigationBranch(branch, true));
  const focusedId = document.activeElement?.dataset.docId;
  documents.replaceChildren(list);
  if (focusedId) [...documents.querySelectorAll(".doc-link")].find(link => link.dataset.docId === focusedId)?.focus({ preventScroll: true });
  else if (focusedDisclosure) {
    [...documents.querySelectorAll("summary[data-disclosure-key]")]
      .find(summary => summary.dataset.disclosureKey === focusedDisclosure)
      ?.focus({ preventScroll: true });
  }
}

function navigationTree(documents) {
  const root = { children: [], childById: new Map(), documents: [] };
  for (const entry of documents) {
    let parent = root;
    for (const segment of entry.navigation) {
      let branch = parent.childById.get(segment.id);
      if (!branch) {
        branch = {
          segment,
          children: [],
          childById: new Map(),
          documents: [],
          containsActive: false,
        };
        parent.childById.set(segment.id, branch);
        parent.children.push(branch);
      }
      if (entry.id === currentId) branch.containsActive = true;
      parent = branch;
    }
    parent.documents.push(entry);
  }
  return root.children;
}

function renderNavigationBranch(branch, topLevel = false) {
  const details = disclosure(
    topLevel ? "doc-group" : "document-folder",
    branch.segment.id,
    branch.segment.label,
    branch.segment.default_open,
  );
  if (branch.containsActive) {
    details.open = true;
    saveDisclosure(branch.segment.id, true);
  }
  for (const entry of branch.documents) details.append(documentLink(entry));
  for (const child of branch.children) details.append(renderNavigationBranch(child));
  return details;
}

function documentLink(entry) {
  const link = node("a", "doc-link", entry.label);
  link.dataset.docId = entry.id;
  link.title = entry.path;
  link.href = locationUrl(entry.id);
  if (entry.id === currentId) link.setAttribute("aria-current", "page");
  return link;
}

function readerMessage(title, description) {
  reader.style.removeProperty("--list-composer-space");
  article.hidden = elements["doc-toolbar"].hidden = true;
  targets.reset();
  const empty = elements["reader-empty"];
  empty.hidden = false;
  empty.replaceChildren(node("h1", "", title));
  if (description) empty.append(node("p", "", description));
}

function headingNode(id) {
  return [...article.querySelectorAll("h1[id],h2[id],h3[id],h4[id],h5[id],h6[id]")].find(heading => heading.id === id);
}

function renderPage() {
  elements["reader-empty"].hidden = true;
  article.hidden = elements["doc-toolbar"].hidden = false;
  elements["current-path"].textContent = entries.find(entry => entry.id === currentId)?.path || "";
  article.innerHTML = page.html; // Produced by the server's escaped Markdown renderer.
  if (!article.textContent.trim()) article.append(node("p", "muted", "This document is empty."));
  for (const table of article.querySelectorAll("table")) {
    const wrapper = node("div", "table-scroll");
    table.replaceWith(wrapper);
    wrapper.append(table);
  }
  hoverTarget = keyboardLine = undefined;
  keyboardScope = 0;
  targets.reset(page);
  if (anchoredDraft && !draftIsCurrent()) {
    textState("line-error", "Choose a new target in this document to move this draft.");
  }
  controls();
}

async function loadDoc(id, fragment = "", refresh = false) {
  if (!id) {
    documentRequest += 1;
    currentId = undefined;
    page = undefined;
    renderNav();
    readerMessage("No documents to review", "Add a Markdown file to the document source or point TrueSpec at another folder.");
    return;
  }
  const token = ++documentRequest;
  const changed = currentId !== id;
  currentId = id;
  if (changed) {
    page = undefined;
    selection = undefined;
    clearMarker();
    textState("doc-notice");
    renderNav();
    readerMessage("Opening document");
  }
  const previous = page;
  try {
    const result = await request(workspaceApi(`documents/${encodeURIComponent(id)}`));
    // A later navigation owns the reader. Ignore an older request's result.
    if (token !== documentRequest) return false;
    if (previous !== result) {
      page = result;
      if (refresh && previous && previous.revision !== result.revision) {
        const writingHere = (composing?.mode === "new" && composing.doc === id) || anchoredDraft?.doc === id;
        textState("doc-notice", writingHere
          ? "Source changed. Your draft text is still here. Select a new target to save it."
          : "Source updated from disk.");
      }
      selection = undefined;
      clearMarker();
      renderPage();
    }
    if (fragment) {
      headingNode(fragment)?.scrollIntoView({ block: "start" });
    } else if (changed) {
      reader.scrollTop = 0;
      if (window.matchMedia("(max-width: 43rem)").matches) reader.scrollIntoView({ block: "start" });
    }
    return true;
  } catch (error) {
    if (token !== documentRequest) return false;
    page = undefined;
    selection = undefined;
    readerMessage("Cannot open this document", error.message);
    controls();
    return false;
  }
}

async function openDoc(id, fragment = "") {
  history.pushState(null, "", locationUrl(id, fragment));
  const result = await loadDoc(id, fragment);
  if (!wideLayout.matches) setPanel("documents", false, { remember: false });
  return result;
}

function captureSelection() {
  if (article.hidden || !page) return;
  const selected = window.getSelection();
  if (!selected?.rangeCount || selected.isCollapsed) {
    selection = undefined;
    controls();
    return;
  }
  const range = selected.getRangeAt(0);
  if (!article.contains(range.startContainer) || !article.contains(range.endContainer)) {
    selection = undefined;
    controls();
    return;
  }
  let start;
  let end;
  for (const span of article.querySelectorAll("[data-run]")) {
    const text = span.firstChild;
    if (!text || text.nodeType !== Node.TEXT_NODE || !range.intersectsNode(text)) continue;
    const left = range.startContainer === text ? range.startOffset : 0;
    const right = range.endContainer === text ? range.endOffset : text.length;
    if (left === right) continue;
    start ??= { run: Number(span.dataset.run), offset: left };
    end = { run: Number(span.dataset.run), offset: right };
  }
  selection = start && end && selected.toString().trim() ? {
    target: { kind: "selection", start, end },
    label: `"${abbreviate(selected.toString().trim(), 120)}"`,
  } : undefined;
  controls();
}

function scheduleSelectionAction() {
  if (selectionFrame) return;
  selectionFrame = requestAnimationFrame(() => {
    selectionFrame = 0;
    positionSelectionAction();
  });
}

function positionSelectionAction() {
  const button = elements["comment-selection"];
  const selected = window.getSelection();
  const writing = !!(composing || anchoredDraft);
  if (!annotationEnabled || !selection || writing || busy || !page || article.hidden || !board || board.review.ended
      || !selected?.rangeCount || selected.isCollapsed) {
    button.hidden = true;
    return;
  }
  const bounds = targets.bounds();
  const rects = [...selected.getRangeAt(0).getClientRects()].filter(rect =>
    rect.width > 0 && rect.height > 0 && rect.right > bounds.left && rect.left < bounds.right
      && rect.bottom > bounds.top && rect.top < bounds.bottom);
  if (!rects.length) {
    button.hidden = true;
    return;
  }
  const selectedBox = {
    left: Math.max(bounds.left, Math.min(...rects.map(rect => rect.left))),
    right: Math.min(bounds.right, Math.max(...rects.map(rect => rect.right))),
    top: Math.max(bounds.top, Math.min(...rects.map(rect => rect.top))),
    bottom: Math.min(bounds.bottom, Math.max(...rects.map(rect => rect.bottom))),
  };
  button.hidden = false;
  const box = button.getBoundingClientRect();
  const gap = 8;
  const above = selectedBox.top - box.height - gap;
  const below = selectedBox.bottom + gap;
  const canAbove = above >= bounds.top;
  const canBelow = below + box.height <= bounds.bottom;
  const preferBelow = window.matchMedia("(max-width: 43rem)").matches;
  let top;
  if (preferBelow && canBelow) top = below;
  else if (canAbove) top = above;
  else if (canBelow) top = below;
  else {
    button.hidden = true;
    return;
  }
  const centered = (selectedBox.left + selectedBox.right - box.width) / 2;
  const left = Math.max(bounds.left + gap, Math.min(centered, bounds.right - box.width - gap));
  button.style.left = `${left}px`;
  button.style.top = `${top}px`;
}

function clearMarker() {
  markedTarget = undefined;
  scheduleLineOutline();
  CSS.highlights?.delete("truespec-target");
  for (const item of article.querySelectorAll(".target-focus")) item.classList.remove("target-focus");
}

function mark(target) {
  clearMarker();
  if (target.kind !== "selection" && !(target.kind === "section" && !target.heading)) {
    hoverTarget = keyboardLine = undefined;
    keyboardScope = 0;
    markedTarget = target;
    targets.element(target)?.scrollIntoView({ block: "center", inline: "nearest" });
    scheduleLineOutline();
    return;
  }
  if (target.kind === "section") {
    article.firstElementChild?.classList.add("target-focus");
    article.firstElementChild?.scrollIntoView({ block: "center" });
    return;
  }
  const first = article.querySelector(`[data-run="${target.start.run}"]`);
  const last = article.querySelector(`[data-run="${target.end.run}"]`);
  if (!first?.firstChild || !last?.firstChild) return;
  if (CSS.highlights && typeof Highlight !== "undefined") {
    const range = new Range();
    range.setStart(first.firstChild, target.start.offset);
    range.setEnd(last.firstChild, target.end.offset);
    CSS.highlights.set("truespec-target", new Highlight(range));
  } else {
    first.classList.add("target-focus");
  }
  first.scrollIntoView({ block: "center" });
}

async function jump(id) {
  const item = board?.comments.find(comment => comment.id === id);
  if (!item?.location.target) return;
  if (!(await openDoc(item.document_id))) return;
  await updateBoard();
  const fresh = board?.comments.find(comment => comment.id === id);
  if (fresh?.location.target && page?.revision === fresh.source_revision) {
    mark(fresh.location.target);
    if (!wideLayout.matches) setPanel("review", false, { remember: false });
  } else {
    textState("doc-notice", "The source changed. This annotation shows its original context in the Review panel.");
  }
}

function abbreviate(text, length) {
  const chars = [...text];
  return chars.length > length ? `${chars.slice(0, length - 1).join("")}…` : text;
}

function caption(item) {
  const { quote, heading, original } = item.anchor;
  if (original.kind === "general") return "General feedback";
  if (original.kind === "line") {
    const line = item.location.target?.line ?? original.line;
    return `Line ${line} · "${abbreviate(quote, 100)}"`;
  }
  if (original.kind === "paragraph" || original.kind === "list") {
    const scope = original.kind === "paragraph" ? "Paragraph" : "List";
    return `${scope} · "${abbreviate(quote.replace(/\s+/g, " "), 100)}"`;
  }
  if (original.kind === "section" && original.heading) return `Section · ${heading.at(-1) || "Untitled"}`;
  if (quote) return `"${abbreviate(quote, 100)}"`;
  return heading.at(-1) || "Entire document";
}

function statusBadge(label, type, description) {
  const badge = node("span", `status ${type || ""}`.trim(), label);
  if (description) {
    badge.title = description;
    badge.setAttribute("aria-label", `${label}. ${description}`);
  }
  return badge;
}

function deliveryIndicator(item) {
  const done = item.delivery === "replied" || item.delivery === "completed";
  const retry = item.delivery === "retry" || item.delivery === "connector_retry";
  const label = {
    queued: item.connector ? `Queued for ${item.connector}` : "Waiting for agent",
    received: "Agent working",
    delivered: "Agent working",
    retry: "Waiting for agent retry",
    connector_retry: "Retrying agent delivery",
    replied: "Agent done",
    completed: "Agent done",
  }[item.delivery];
  const indicator = node(
    "span",
    `delivery-indicator ${done ? "done" : "working"}${retry ? " warning" : ""}`,
  );
  indicator.setAttribute("role", "status");
  indicator.setAttribute("aria-label", label);
  indicator.title = label;
  if (done) {
    indicator.innerHTML =
      '<svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5.25 10.3 3.05 3.05 6.45-6.7" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>';
  } else {
    indicator.append(node("span", "delivery-spinner"));
  }
  return indicator;
}

function renderAgentReply(reply) {
  const row = node("div", "annotation agent-reply");
  row.dataset.replyId = reply.id;
  row.append(node("p", "agent-reply-author", "Agent reply"));
  const body = node("div", "agent-reply-body");
  body.innerHTML = reply.html;
  row.append(body);
  const linked = reply.comment_ids
    .map(id => board.comments.find(comment => comment.id === id))
    .filter(Boolean);
  if (linked.length) {
    const references = node("details", "reply-references");
    references.append(node("summary", "", `Regarding ${linked.length} ${linked.length === 1 ? "comment" : "comments"}`));
    for (const comment of linked) {
      if (comment.anchor.original.kind === "general") {
        references.append(node("span", "reply-reference static", "General feedback"));
        continue;
      }
      const link = makeButton(
        `${comment.path} · ${caption(comment)}`,
        "reply-reference",
        () => void jump(comment.id).catch(problem),
        `${reply.id}:${comment.id}`,
      );
      link.disabled = !comment.location.target;
      references.append(link);
    }
    row.append(references);
  }
  return row;
}

function replySeenKey() {
  return `truespec.reply-seen.${reviewId}`;
}

function maxReplySequence() {
  return Math.max(0, ...(board?.replies ?? []).map(reply => reply.sequence));
}

function markRepliesSeen() {
  const sequence = maxReplySequence();
  if (!sequence) return;
  try { sessionStorage.setItem(replySeenKey(), String(sequence)); }
  catch { /* Read state is tab-local and may be unavailable in private browsing. */ }
  elements["review-toggle"].classList.remove("is-unread");
}

function updateReplyUnread() {
  let seen = 0;
  try { seen = Number(sessionStorage.getItem(replySeenKey()) || 0); }
  catch { /* Treat replies as unread when tab storage is unavailable. */ }
  const unread = maxReplySequence() > seen;
  elements["review-toggle"].classList.toggle("is-unread", unread && !reviewOpen);
  if (reviewOpen && !document.hidden) markRepliesSeen();
}

function renderReview() {
  if (!board) return;
  const readonly = board.review.ended;
  const connector = board.review.connector;
  elements["shortcut-count"].textContent = String(board.comments.length);
  elements.ended.hidden = !readonly;
  elements["ended-note"].textContent = connector
    ? `This browser review has ended. Queued batches can still reach ${connector}. Start another review from your coding chat.`
    : "This browser review has ended. Any feedback you sent stays in the local inbox.";
  elements["new-review"].hidden = !!connector;
  elements["new-review"].textContent = board.latest === reviewId ? "Start a new review" : "Open current review";
  elements.handoff.hidden = readonly;
  const list = elements["review-list"];
  const focusKey = document.activeElement?.dataset.focusKey;
  const items = [
    ...board.comments.map(comment => ({ kind: "comment", sequence: comment.sequence, value: comment })),
    ...(board.replies ?? []).map(reply => ({ kind: "reply", sequence: reply.sequence, value: reply })),
  ].sort((left, right) => left.sequence - right.sequence);
  const fragment = document.createDocumentFragment();
  if (!items.length) {
    const empty = node("div", "review-empty");
    empty.append(node("h3", "", "No comments yet"));
    empty.append(node("p", "",
      "Click text for a line, or use the left margin for a paragraph, list, or section. You can also select exact text."));
    fragment.append(empty);
  }
  for (const entry of items) {
    if (entry.kind === "reply") {
      fragment.append(renderAgentReply(entry.value));
      continue;
    }
    const item = entry.value;
    const row = node("div", item.batch_id ? "annotation" : "annotation draft");
    row.dataset.commentId = item.id;
    const general = item.anchor.original.kind === "general";
    if (!general) {
      row.append(node("p", "annotation-path", item.path));
      const link = makeButton(caption(item), "target-link", () => void jump(item.id).catch(problem), `${item.id}:jump`);
      link.disabled = !item.location.target;
      link.title = item.anchor.quote || item.anchor.heading.join(" / ") || item.path;
      row.append(link);
    }
    row.append(node("p", "", item.body));
    if (!general && (item.anchor.path !== item.path || item.anchor.quote || item.location.status === "needs_review")) {
      const evidence = node("details", "original-context");
      evidence.append(node("summary", "", "Original context"));
      if (item.anchor.original.kind === "line" || item.anchor.path !== item.path) {
        const originalLine = item.anchor.original.kind === "line" ? `:${item.anchor.original.line}` : "";
        evidence.append(node("p", "", `${item.anchor.path}${originalLine}`));
      }
      if (item.anchor.heading.length) evidence.append(node("p", "", item.anchor.heading.join(" / ")));
      if (item.anchor.quote) evidence.append(node("blockquote", "", item.anchor.quote));
      else if (item.anchor.after) evidence.append(node("blockquote", "", item.anchor.after));
      row.append(evidence);
    }
    const badges = node("div", "annotation-status");
    if (item.delivery) badges.append(deliveryIndicator(item));
    if (item.location.status === "needs_review") {
      badges.append(statusBadge(
        "Outdated",
        "warning",
        "The document changed, so this comment may point to the wrong place.",
      ));
    }
    if (item.location.status === "missing") badges.append(statusBadge("Source missing", "warning"));
    if (item.location.status === "relocated") badges.append(statusBadge("Relocated", "neutral"));
    if (badges.childElementCount) row.append(badges);
    if (!readonly && !item.batch_id) {
      const actions = node("div", "annotation-actions");
      actions.append(
        makeButton("Edit", "text-button", () => editComposer(item), `${item.id}:edit`),
        makeButton("Remove", "text-button", () => void changeItem(item, "remove"), `${item.id}:remove`));
      row.append(actions);
    }
    fragment.append(row);
  }
  list.replaceChildren(fragment);
  if (focusKey) [...list.querySelectorAll("[data-focus-key]")].find(el => el.dataset.focusKey === focusKey)?.focus({ preventScroll: true });
  controls();
  updateReplyUnread();
}

function controls() {
  const readonly = !board || board.review.ended;
  const count = board?.comments.filter(item => !item.batch_id).length ?? 0;
  const pendingGeneral = !!generalComment.value.trim();
  const writing = !!(composing || anchoredDraft);
  elements.send.disabled = readonly || busy || writing || !(count || pendingGeneral);
  elements["send-end"].disabled = readonly || busy || writing;
  elements["annotation-toggle"].hidden = readonly;
  elements["annotation-toggle"].disabled = readonly || busy || writing;
  elements["annotation-toggle"].setAttribute("aria-checked", String(annotationEnabled));
  elements["comment-selection"].disabled = readonly || busy || !annotationEnabled || !!anchoredDraft || !selection;
  article.tabIndex = annotationEnabled && !readonly && targets.numbers.length ? 0 : -1;
  if (article.tabIndex === 0) article.setAttribute("aria-describedby", "line-keyboard");
  else article.removeAttribute("aria-describedby");
  lineComment.readOnly = busy || readonly;
  generalComposer.hidden = readonly;
  generalComment.disabled = readonly || busy;
  generalComposerToggle.disabled = readonly || busy;
  updateGeneralComposerDisclosure();
  elements["cancel-line"].disabled = busy;
  elements["save-comment"].disabled = busy || readonly;
  elements["cancel-comment"].disabled = busy;
  elements["new-review"].disabled = busy || writing;
  scheduleLineOutline();
  scheduleSelectionAction();
}

function toggleAnnotation() {
  if (busy || composing || anchoredDraft || !board || board.review.ended) return;
  annotationEnabled = !annotationEnabled;
  hoverTarget = undefined;
  keyboardLine = undefined;
  keyboardScope = 0;
  if (!annotationEnabled) {
    clearMarker();
    article.blur();
  }
  controls();
}

function openComposer({ target, label }) {
  if (!annotationEnabled || !board || board.review.ended || !page || busy || anchoredDraft) return;
  if (composing?.mode === "edit") {
    textState("error", "Save or cancel the draft you are editing before starting another comment.");
    comment.focus();
    return;
  }
  if (composing?.mode === "new" && composing.revision !== page.revision) textState("doc-notice");
  if (!composing) comment.value = "";
  composing = { mode: "new", doc: currentId, revision: page.revision, target };
  elements["composer-label"].textContent = "New draft";
  elements["composer-context"].textContent = label;
  showComposer();
}

function editComposer(item) {
  if (busy) return;
  if (anchoredDraft) {
    textState("line-error", "Queue or cancel this draft first.");
    lineComment.focus({ preventScroll: true });
    return;
  }
  if (composing) {
    textState("error", "Save or cancel the open draft first.");
    comment.focus();
    return;
  }
  composing = { mode: "edit", id: item.id, version: item.version };
  comment.value = item.body;
  elements["composer-label"].textContent = "Edit draft";
  elements["composer-context"].textContent = caption(item);
  showComposer();
}

function showComposer() {
  renderReview();
  composer.hidden = false;
  if (!wideLayout.matches) setPanel("review", true, { remember: false });
  textState("error");
  textState("status");
  comment.focus();
}

function closeComposer() {
  comment.value = "";
  composing = undefined;
  composer.hidden = true;
  controls();
}

function canAnnotate() {
  return annotationEnabled && !!board && !board.review.ended && !!page;
}

function draftIsCurrent() {
  return anchoredDraft && anchoredDraft.doc === currentId && anchoredDraft.revision === page?.revision;
}

function scheduleLineOutline() {
  if (outlineFrame) return;
  outlineFrame = requestAnimationFrame(() => {
    outlineFrame = 0;
    positionLineUi();
  });
}

function positionLineUi() {
  const editable = canAnnotate();
  const active = editable && draftIsCurrent() ? anchoredDraft.target : undefined;
  const keys = keyboardLine === undefined ? undefined : targets.scopesForLine(keyboardLine)[keyboardScope];
  const outline = active ?? (editable ? hoverTarget ?? keys ?? markedTarget : markedTarget);
  const boxes = targets.draw(outline, active !== undefined);
  if (anchoredDraft) placeLineComposer(active ? boxes : [], !draftIsCurrent());
}

function placeLineComposer(boxes, detached) {
  const viewport = viewportBounds();
  const reader = article.closest(".reader").getBoundingClientRect();
  const form = lineComposer.getBoundingClientRect();
  const margin = COMPOSER_MARGIN;
  const anchor = boxes.at(-1);
  const minX = Math.max(reader.left, viewport.left) + margin;
  const maxX = Math.min(reader.right, viewport.right) - form.width - margin;
  const left = Math.max(minX, Math.min(anchor?.left ?? (form.left || minX), maxX));
  const maxY = viewport.bottom - form.height - margin;
  let below = anchor ? anchor.bottom + COMPOSER_GAP : (detached ? maxY : form.top || maxY);
  const target = anchoredDraft.target;
  if (anchor && below > maxY && target.kind !== "line") {
    const element = targets.element(target)?.getBoundingClientRect();
    const clickY = element ? element.top + anchoredDraft.offset : boxes[0].top;
    below = Math.min(maxY, Math.max(boxes[0].top + COMPOSER_GAP, clickY + COMPOSER_GAP));
  }
  const above = boxes[0]?.top - form.height - COMPOSER_GAP;
  const top = Math.max(viewport.top + margin, Math.min(below > maxY && above >= viewport.top + margin ? above : below, maxY));
  lineComposer.style.left = `${left}px`;
  lineComposer.style.top = `${top}px`;
}

function openAnchoredComposer(target, pointY) {
  const anchor = targets.element(target);
  if (!canAnnotate() || busy || !anchor) return;
  if (composing) {
    textState("error", "Save or cancel the open draft first.");
    comment.focus();
    return;
  }
  if (anchoredDraft && !draftIsCurrent()) textState("doc-notice");
  if (!anchoredDraft) lineComment.value = "";
  anchoredDraft = { doc: currentId, revision: page.revision, target, offset: pointY === undefined ? 0 : pointY - anchor.getBoundingClientRect().top };
  reader.style.removeProperty("--list-composer-space");
  elements["line-label"].textContent = targets.caption(target);
  textState("line-error");
  textState("status");
  lineComposer.hidden = false;
  makeComposerRoom(target, anchor);
  controls();
  positionLineUi();
  lineComment.focus({ preventScroll: true });
}

function makeComposerRoom(target, anchor) {
  const boxes = targets.rectangles(target);
  if (!boxes.length) return;
  const viewport = viewportBounds();
  const bottom = boxes.at(-1).bottom;
  const height = lineComposer.getBoundingClientRect().height;
  const needed = Math.ceil(bottom + height + COMPOSER_GAP + COMPOSER_MARGIN - viewport.bottom);
  if (needed <= 0) return;
  if (target.kind === "line") {
    anchor.scrollIntoView({ block: "center", inline: "nearest" });
    return;
  }
  const area = targets.bounds();
  if (bottom >= area.bottom - 2 || boxes[0].top - needed < area.top + 6) return;
  const scroller = getComputedStyle(reader).overflowY === "auto" ? reader : window;
  if (target.kind === "list" && scroller === reader) {
    const remaining = reader.scrollHeight - reader.clientHeight - reader.scrollTop;
    if (needed > remaining) reader.style.setProperty("--list-composer-space", `${Math.ceil(needed - remaining)}px`);
  }
  scroller.scrollBy({ top: needed, behavior: "instant" });
}

function closeLineComposer() {
  const previous = draftIsCurrent() ? anchoredDraft.target : undefined;
  anchoredDraft = undefined;
  reader.style.removeProperty("--list-composer-space");
  lineComposer.hidden = true;
  lineComment.value = "";
  textState("line-error");
  if (canAnnotate()) {
    keyboardLine = previous?.line;
    keyboardScope = 0;
    article.focus({ preventScroll: true });
  }
  controls();
}

function moveKeyboardTarget(event) {
  if (event.target !== article || !canAnnotate() || busy || event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return;
  const numbers = targets.numbers;
  if (!numbers.length) return;
  const isArrow = event.key === "ArrowUp" || event.key === "ArrowDown";
  const changeScope = event.key === "ArrowLeft" || event.key === "ArrowRight";
  if (!isArrow && !changeScope && event.key !== "Enter") return;
  event.preventDefault();
  const index = numbers.indexOf(keyboardLine);
  const direction = event.key === "ArrowUp" ? -1 : 1;
  keyboardLine = index < 0
    ? numbers.find(line => targets.lineRectangles(line).length) ?? numbers[0]
    : numbers[Math.max(0, Math.min(index + (isArrow ? direction : 0), numbers.length - 1))];
  const scopes = targets.scopesForLine(keyboardLine);
  if (isArrow) keyboardScope = 0;
  else if (changeScope) keyboardScope = Math.max(0, Math.min(keyboardScope + (event.key === "ArrowRight" ? 1 : -1), scopes.length - 1));
  const target = scopes[keyboardScope];
  if (!target) return;
  hoverTarget = undefined;
  if (isArrow || index < 0) targets.element(scopes[0])?.scrollIntoView({ block: "nearest", inline: "nearest" });
  textState("line-focus", targets.caption(target));
  scheduleLineOutline();
  if (event.key === "Enter") openAnchoredComposer(target);
}

async function queueLineDraft() {
  const draft = anchoredDraft;
  const body = lineComment.value.trim();
  if (!draft || !body || !canAnnotate()) return;
  if (!draftIsCurrent()) {
    textState("line-error", "Choose a new target in this document to move this draft.");
    return;
  }
  await action(async () => {
    textState("line-error");
    try {
      await request(`${workspacePath()}/comments`, {
        document_id: draft.doc, revision: draft.revision, target: draft.target, body,
      });
    } catch (error) {
      textState("line-error", error instanceof TypeError ? "Could not queue the draft. Check the TrueSpec server and try again." : error.message);
      return;
    }
    closeLineComposer();
    await updateBoard();
    textState("status", "Comment queued.");
  });
}

async function queueGeneralFeedback() {
  const body = generalComment.value.trim();
  if (!body || busy || !board || board.review.ended) return;
  let queued = false;
  await action(async () => {
    await request(`${workspacePath()}/general`, { body });
    generalComment.value = "";
    await updateBoard();
    textState("status", "Comment queued.");
    queued = true;
  });
  if (queued) generalComment.focus();
}

async function action(callback) {
  if (busy) return;
  busy = true;
  textState("error");
  textState("status");
  controls();
  try {
    await callback();
  } catch (error) {
    problem(error);
  } finally {
    busy = false;
    controls();
  }
}

function ask(title, message, actionLabel) {
  elements["confirm-heading"].textContent = title;
  elements["confirm-copy"].textContent = message;
  elements["confirm-action"].textContent = actionLabel;
  confirmation.returnValue = "";
  confirmation.showModal();
  return new Promise(resolve => confirmation.addEventListener("close", () => resolve(confirmation.returnValue === "confirm"), { once: true }));
}

async function changeItem(item, operation, extra = {}) {
  await action(async () => {
    if (operation === "remove" && !(await ask("Remove this draft?", "The draft has not been sent.", "Remove draft"))) return;
    await request(`${workspacePath()}/comments/${encodeURIComponent(item.id)}`, { action: operation, version: item.version, ...extra });
    if (operation === "remove" && composing?.id === item.id) closeComposer();
    await updateBoard();
  });
}

async function sendDrafts(end) {
  await action(async () => {
    const drafts = board.comments.filter(item => !item.batch_id).map(item => ({ id: item.id, version: item.version }));
    const generalBody = generalComment.value.trim() || null;
    if (!drafts.length && !generalBody) {
      if (!end) return;
      await request(`${workspacePath()}/end`, { keep_drafts: false });
      await updateBoard();
      focusEnded();
      return;
    }
    const signature = JSON.stringify([reviewId, end, drafts, generalBody]);
    if (lastSubmit?.signature !== signature) {
      lastSubmit = {
        signature,
        key: crypto.randomUUID(),
        drafts,
        generalBody,
        end,
        confirmation: null,
      };
    }
    const payload = {
      key: lastSubmit.key,
      drafts,
      general_body: lastSubmit.generalBody,
      end,
      confirmation: lastSubmit.confirmation,
    };
    let result;
    try {
      result = await request(`${workspacePath()}/submit`, payload);
    } catch (error) {
      if (!error.confirmation || !(await ask("Send original context?", error.message, end ? "Send and End" : "Send"))) {
        if (!error.confirmation) throw error;
        return;
      }
      payload.confirmation = lastSubmit.confirmation = error.confirmation;
      result = await request(`${workspacePath()}/submit`, payload);
    }
    lastSubmit = undefined;
    if (generalBody) generalComment.value = "";
    await updateBoard();
    const destination = board.review.connector || "the agent";
    textState("status", `${result.count} ${result.count === 1 ? "comment" : "comments"} queued for ${destination}.`);
    if (end) focusEnded();
  });
}

function focusEnded() {
  const title = elements.ended.querySelector("h3");
  title.tabIndex = -1;
  title.focus();
}

function focusReview() {
  const heading = elements["review-heading"];
  heading.tabIndex = -1;
  heading.focus();
}

async function startReview() {
  await action(async () => {
    const review = board.latest === reviewId
      ? await request(`${workspacePath()}/new`, {})
      : (await request(workspaceApi("bootstrap"))).review;
    reviewId = review.id;
    lastSubmit = undefined;
    history.replaceState(null, "", locationUrl(currentId, currentFragment()));
    await updateBoard();
    renderNav();
    focusReview();
  });
}

async function updateBoard() {
  const id = reviewId;
  const refreshed = await request(workspacePath());
  if (id !== reviewId || board === refreshed) return;
  board = refreshed;
  renderReview();
}

async function updateCatalog() {
  const refreshed = await request(workspaceApi("documents"));
  if (entries !== refreshed) {
    entries = refreshed;
    renderNav();
  }
}

async function refresh() {
  if (document.hidden || refreshing || busy || !reviewId || !board) return;
  refreshing = true;
  try {
    await updateCatalog();
    if (currentId) await loadDoc(currentId, "", true);
    else if (entries.length) await openDoc(entries[0].id);
    await updateBoard();
  } catch (error) {
    problem(error);
  } finally {
    refreshing = false;
  }
}

function internalLink(event, link) {
  if (!link || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  const href = new URL(link.href);
  const pathname = workspaceId ? `/w/${encodeURIComponent(workspaceId)}/` : "/";
  if (href.origin !== window.location.origin || href.pathname !== pathname) return;
  const id = href.searchParams.get("doc");
  if (!id && !href.hash) return;
  let heading = "";
  try { heading = decodeURIComponent(href.hash.slice(1)); } catch { return; }
  event.preventDefault();
  if (id && id !== currentId) void openDoc(id, heading).catch(problem);
  else if (heading) {
    history.pushState(null, "", locationUrl(currentId, heading));
    headingNode(heading)?.scrollIntoView({ block: "start" });
  }
}

documents.addEventListener("click", event => internalLink(event, event.target.closest(".doc-link")));
elements["documents-toggle"].addEventListener("click", () =>
  setPanel("documents", true, { focus: true }));
elements["review-toggle"].addEventListener("click", () =>
  setPanel("review", true, { focus: true }));
elements["review-expand"].addEventListener("click", () =>
  setReviewExpanded(!reviewExpanded));
elements["review-resizer"].addEventListener("pointerdown", event => {
  if (event.button !== 0 || !reviewResizeEnabled()) return;
  event.preventDefault();
  reviewDrag = { pointerId: event.pointerId };
  elements.layout.dataset.reviewResizing = "true";
  try { elements["review-resizer"].setPointerCapture(event.pointerId); }
  catch { /* Synthetic pointers used by tests cannot be captured. */ }
  resizeReviewFromPointer(event);
});
elements["review-resizer"].addEventListener("pointermove", event => {
  if (reviewDrag?.pointerId === event.pointerId) resizeReviewFromPointer(event);
});
elements["review-resizer"].addEventListener("pointerup", finishReviewResize);
elements["review-resizer"].addEventListener("pointercancel", finishReviewResize);
elements["review-resizer"].addEventListener("lostpointercapture", finishReviewResize);
elements["review-resizer"].addEventListener("keydown", event => {
  if (!reviewResizeEnabled()) return;
  if (event.key === "Home") {
    event.preventDefault();
    reviewWidthPreference = undefined;
    saveReviewWidth(undefined);
    applyReviewWidth();
    return;
  }
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  event.preventDefault();
  const step = event.shiftKey ? 48 : 16;
  const current = reviewWidthPreference ?? elements.review.getBoundingClientRect().width;
  reviewWidthPreference = clampReviewWidth(
    current + (event.key === "ArrowLeft" ? step : -step),
  );
  saveReviewWidth(reviewWidthPreference);
  applyReviewWidth();
});
elements["documents-close"].addEventListener("click", () => {
  setPanel("documents", false);
  requestAnimationFrame(() => elements["documents-toggle"].focus());
});
elements["review-close"].addEventListener("click", () => {
  setPanel("review", false);
  requestAnimationFrame(() => elements["review-toggle"].focus());
});
elements["sidebar-backdrop"].addEventListener("click", () => closeOverlay(true));
document.addEventListener("keydown", event => {
  if (event.key === "Escape" && reviewExpanded
      && !confirmation.open && lineComposer.hidden && composer.hidden) {
    event.preventDefault();
    setReviewExpanded(false);
  } else if (event.key === "Escape" && !wideLayout.matches && (documentsOpen || reviewOpen)
      && !confirmation.open && lineComposer.hidden && composer.hidden) {
    event.preventDefault();
    closeOverlay(true);
  }
});
for (const panel of [elements["documents-panel"], elements.review]) {
  panel.addEventListener("transitionend", event => {
    if (event.propertyName === "transform") {
      scheduleLineOutline();
      scheduleSelectionAction();
    }
  });
}
wideLayout.addEventListener("change", resetPanelsForViewport);
const readerControl = "a, button, input, textarea, select, summary";
reader.addEventListener("pointerdown", event => {
  pointerStart = { x: event.clientX, y: event.clientY };
});
reader.addEventListener("pointermove", event => {
  if (event.pointerType === "touch") return;
  const next = canAnnotate() && !event.buttons && !event.target.closest(readerControl)
    ? targets.at(event.clientX, event.clientY, event.target) : undefined;
  if (targetKey(hoverTarget) !== targetKey(next) || keyboardLine !== undefined) {
    hoverTarget = next;
    keyboardLine = undefined;
    keyboardScope = 0;
    scheduleLineOutline();
  }
});
reader.addEventListener("pointerleave", () => {
  hoverTarget = undefined;
  scheduleLineOutline();
});
reader.addEventListener("click", event => {
  const dragged = pointerStart && Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y) > 5;
  pointerStart = undefined;
  const control = event.target.closest(readerControl);
  if (control) {
    if (control.matches("a")) internalLink(event, control);
    return;
  }
  if (dragged || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey
      || !canAnnotate() || getSelection()?.isCollapsed === false) return;
  const target = targets.at(event.clientX, event.clientY, event.target);
  if (target) openAnchoredComposer(target, event.clientY);
});
article.addEventListener("keydown", moveKeyboardTarget);
article.addEventListener("blur", () => {
  keyboardLine = undefined;
  keyboardScope = 0;
  scheduleLineOutline();
});
lineComposer.addEventListener("submit", event => {
  event.preventDefault();
  void queueLineDraft();
});
lineComposer.addEventListener("keydown", event => {
  if (event.isComposing) return;
  if (event.key === "Escape" && !busy) {
    event.preventDefault();
    closeLineComposer();
  } else if (event.target === lineComment && event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    void queueLineDraft();
  }
});
generalComposer.addEventListener("submit", event => {
  event.preventDefault();
  void queueGeneralFeedback();
});
generalComposerToggle.addEventListener("click", toggleGeneralComposer);
generalComment.addEventListener("input", controls);
generalComment.addEventListener("keydown", event => {
  if (!event.isComposing && event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    void queueGeneralFeedback();
  }
});
elements["cancel-line"].addEventListener("click", closeLineComposer);
window.addEventListener("scroll", scheduleLineOutline, { passive: true, capture: true });
window.addEventListener("scroll", scheduleSelectionAction, { passive: true, capture: true });
window.addEventListener("resize", scheduleLineOutline, { passive: true });
window.addEventListener("resize", scheduleSelectionAction, { passive: true });
window.addEventListener("resize", applyReviewWidth, { passive: true });
window.visualViewport?.addEventListener("resize", scheduleLineOutline, { passive: true });
window.visualViewport?.addEventListener("resize", scheduleSelectionAction, { passive: true });
window.visualViewport?.addEventListener("scroll", scheduleLineOutline, { passive: true });
window.visualViewport?.addEventListener("scroll", scheduleSelectionAction, { passive: true });
document.addEventListener("selectionchange", captureSelection);
elements["comment-selection"].addEventListener("pointerdown", event => event.preventDefault());
elements["comment-selection"].addEventListener("click", () => { if (selection) openComposer(selection); });
elements["annotation-toggle"].addEventListener("pointerdown", event => event.preventDefault());
elements["annotation-toggle"].addEventListener("click", toggleAnnotation);
elements["cancel-comment"].addEventListener("click", () => {
  closeComposer();
  textState("error");
  focusReview();
});
composer.addEventListener("submit", event => {
  event.preventDefault();
  const draft = composing;
  const body = comment.value.trim();
  if (!draft || !body) return;
  void action(async () => {
    if (draft.mode === "edit") {
      await request(`${workspacePath()}/comments/${encodeURIComponent(draft.id)}`, { action: "edit", body, version: draft.version });
    } else {
      await request(`${workspacePath()}/comments`, { body, document_id: draft.doc, revision: draft.revision, target: draft.target });
    }
    if (draft.mode === "new" && draft.target.kind === "selection") {
      window.getSelection()?.removeAllRanges();
      selection = undefined;
    }
    closeComposer();
    await updateBoard();
    textState("status", "Comment queued.");
    focusReview();
  });
});
elements["send-end"].addEventListener("click", () => void sendDrafts(true));
elements.send.addEventListener("click", () => void sendDrafts(false));
elements["new-review"].addEventListener("click", () => void startReview());
window.addEventListener("popstate", () => {
  const params = new URL(window.location.href).searchParams;
  const targetReview = params.get("review");
  if (targetReview && targetReview !== reviewId) return window.location.reload();
  void loadDoc(params.get("doc") || entries[0]?.id, currentFragment()).catch(problem);
});
document.addEventListener("visibilitychange", () => void refresh());

async function initialize() {
  const bootstrap = await request(workspaceApi("bootstrap"));
  const params = new URL(window.location.href).searchParams;
  reviewId = params.get("review") || bootstrap.review.id;
  resetPanelsForViewport();
  await updateCatalog();
  const id = params.get("doc") || entries[0]?.id;
  history.replaceState(null, "", locationUrl(id, currentFragment()));
  await Promise.all([loadDoc(id, currentFragment()), updateBoard()]);
  window.setInterval(() => void refresh(), 1800);
}

resetPanelsForViewport();
void initialize().catch(error => {
  readerMessage("TrueSpec could not start", error.message);
  problem(error);
});
