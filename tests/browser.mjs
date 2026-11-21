import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { once } from "node:events";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";

const cli = resolve(process.env.TRUESPEC_BIN || "target/debug/truespec");
const chromeEnv = { ...process.env, CHROME_DEVTOOLS_AXI_SESSION: `truespec-test-${process.pid}` };
const screenshots = process.env.TRUESPEC_SCREENSHOTS;

function call(binary, args, options = {}) {
  const result = spawnSync(binary, args, { encoding: "utf8", timeout: 90_000, ...options });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${binary} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

function chrome(args, input) {
  return call("chrome-devtools-axi", args, { env: chromeEnv, input });
}

function script(input) {
  return chrome(["run"], `
    const check = (condition, message) => { if (!condition) throw new Error(message); };
    const showPanel = async name => {
      const toggle = "#" + name + "-toggle";
      if (await page.eval("() => document.querySelector(" + JSON.stringify(toggle) + ").getAttribute('aria-expanded') !== 'true'")) {
        await page.click(toggle);
      }
      await page.wait(toggle + "[aria-expanded='true']");
    };
    const hidePanel = async name => {
      const toggle = "#" + name + "-toggle";
      if (await page.eval("() => document.querySelector(" + JSON.stringify(toggle) + ").getAttribute('aria-expanded') === 'true'")) {
        await page.click("#" + name + "-close");
      }
      await page.wait(toggle + "[aria-expanded='false']");
    };
    // AXI's selector click calls element.click() without pointer coordinates.
    // Dispatch at the text's actual client position for the line hit test.
    const clickLine = selector => page.eval("() => (" + ((selector) => {
      const word = document.querySelector(selector);
      word.scrollIntoView({ block: "center", inline: "nearest" });
      const rect = word.getClientRects()[0];
      const coordinates = { bubbles: true, clientX: (rect.left + rect.right) / 2,
        clientY: (rect.top + rect.bottom) / 2, button: 0 };
      word.dispatchEvent(new PointerEvent("pointerdown", coordinates));
      word.dispatchEvent(new MouseEvent("click", coordinates));
    }).toString() + ")(" + JSON.stringify(selector) + ")");
    const margin = (scope, selector, click = false) => page.eval("() => (" + (({ scope, selector, click }) => {
      const block = document.querySelector(selector);
      block.scrollIntoView({ block: "center", inline: "nearest" });
      const rect = block.getBoundingClientRect();
      const clientX = document.querySelector("#article").getBoundingClientRect().left - (scope === "section" ? 14 : 4);
      const clientY = (rect.top + rect.bottom) / 2;
      const hit = document.elementFromPoint(clientX, clientY);
      const coordinates = { bubbles: true, clientX, clientY, button: 0, pointerType: "mouse" };
      hit.dispatchEvent(new PointerEvent("pointermove", coordinates));
      if (click) {
        hit.dispatchEvent(new PointerEvent("pointerdown", coordinates));
        hit.dispatchEvent(new MouseEvent("click", coordinates));
      }
    }).toString() + ")(" + JSON.stringify({ scope, selector, click }) + ")");
    ${input}
  `);
}

async function viewerApi(viewer, path, body, expected = 200) {
  const response = await fetch(new URL(path, viewer.url), body === undefined ? {} : {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Origin: new URL(viewer.url).origin,
    },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, expected, `${path}: ${text}`);
  return text ? JSON.parse(text) : null;
}

async function start(root, mode) {
  const args = mode === "openspec" ? ["serve"] : ["serve", "--source", "markdown", "--docs", "docs"];
  const child = spawn(cli, args, { cwd: root, env: { ...process.env, PATH: "" }, stdio: ["ignore", "pipe", "inherit"] });
  const stream = createInterface({ input: child.stdout });
  try {
    const lines = stream[Symbol.asyncIterator]();
    const url = (await lines.next()).value?.match(/http:\/\/127\.0\.0\.1:\d+\//)?.[0];
    const review = (await lines.next()).value?.match(/^Review: (.*)$/)?.[1];
    assert.ok(url && review, "Viewer should report its loopback URL and review ID");
    return { child, url, review };
  } catch (error) {
    child.kill();
    throw error;
  } finally {
    stream.close();
  }
}

async function stop(viewer) {
  if (!viewer || viewer.child.exitCode !== null || viewer.child.signalCode !== null) return;
  const exit = once(viewer.child, "exit");
  viewer.child.kill("SIGTERM");
  const [code] = await exit;
  assert.equal(code, 0);
}

async function fixture(root, mode) {
  const first = mode === "openspec" ? "openspec/specs/account/spec.md" : "docs/account.md";
  const second = mode === "openspec" ? "openspec/changes/revise-login/design.md" : "docs/session.md";
  const link = mode === "openspec" ? "../../changes/revise-login/design.md" : "session.md";
  const list = mode === "openspec"
    ? "- First goal.\n  - Nested goal.\n- Last goal."
    : "3. First goal.\n   - Nested goal.\n4. Last goal.";
  const source = `# Account policy

This specification describes how accounts sign in and what happens after a session expires.

## Login

Members can use a **one-time token**. A temporary token expires after the period defined by the session policy.
This second authored line belongs to the same paragraph.

| Input | Expected result |
| --- | --- |
| Valid token | Open the workspace |
| Expired token | Ask the member to sign in again |

[Read the session policy](${link}#timing)

## Next topic

This is outside the Login section.

**Goals:**

${list}

This follows the list.
`;
  for (const path of [first, second]) await mkdir(dirname(join(root, path)), { recursive: true });
  await writeFile(join(root, first), source);
  await writeFile(join(root, second), "# Session policy\n\n## Timing\n\nA **formatted** 😀 `token` helps explain the expiry path.\n");
  if (mode === "openspec") {
    await writeFile(join(root, "openspec/changes/revise-login/proposal.md"), "# Proposal\n");
    await writeFile(join(root, "openspec/changes/revise-login/tasks.md"), "# Tasks\n");
    await mkdir(join(root, "openspec/changes/revise-login/specs/account"), { recursive: true });
    await writeFile(join(root, "openspec/changes/revise-login/specs/account/spec.md"), "# Account delta\n");
  }
  return { first, second, source };
}

async function exercise(mode, width) {
  const root = await mkdtemp(join(tmpdir(), `truespec-${mode}-`));
  let viewer;
  try {
    const { first, second, source } = await fixture(root, mode);
    viewer = await start(root, mode);
    chrome(["open", viewer.url]);
    if (width < 500) chrome(["emulate", "--viewport", `${width}x880x1`]);
    else chrome(["resize", String(width), "880"]);
    if (width >= 960) {
      chrome(["resize", "1024", "880"]);
      script(`
        await page.wait("#article:not([hidden])");
        check(await page.eval(() => document.querySelector("#reader").getBoundingClientRect().width > 400),
          "Tablet-landscape desktop columns leave a usable reader");
      `);
      chrome(["resize", "768", "880"]);
      script(`
        await page.wait("#documents-toggle[aria-expanded='false']");
        check(await page.eval(() => document.querySelector("#review-toggle").getAttribute("aria-expanded") === "false"
          && document.querySelector("#reader").getBoundingClientRect().width === innerWidth),
          "Tablet portrait switches both sidebars to closed overlay drawers");
      `);
      chrome(["resize", String(width), "880"]);
    }
    script(`
      await page.wait("#article:not([hidden]) #login");
      await page.wait("#review-heading");
      ${width >= 960 ? `
        await page.wait("#documents-toggle[aria-expanded='true']");
        await page.wait("#review-toggle[aria-expanded='true']");
        const fullReaderWidth = await page.eval(() => document.querySelector("#reader").getBoundingClientRect().width);
        await page.click("#documents-close");
        await page.wait("#documents-toggle[aria-expanded='false']");
        const collapsedReaderWidth = await page.eval(() => document.querySelector("#reader").getBoundingClientRect().width);
        check(await page.eval(() => document.querySelector("#documents-panel").inert)
          && collapsedReaderWidth > fullReaderWidth,
          "Desktop document collapse expands the reader and removes hidden controls from focus");
        await page.click("#documents-toggle");
        check(await page.eval(async () => {
          for (let attempt = 0; attempt < 10 && document.activeElement.id !== "documents-close"; attempt++) {
            await new Promise(resolve => setTimeout(resolve, 20));
          }
          return document.activeElement.id === "documents-close";
        }), "Reopening Documents moves focus to its panel control");
        await page.click("#review-close");
        await page.wait("#review-toggle[aria-expanded='false']");
        check(await page.eval(() => document.querySelector("#review").inert
          && localStorage.getItem("truespec.sidebar.review") === "false"),
          "Desktop review collapse persists its preference");
        await page.click("#review-toggle");
        check(await page.eval(async () => {
          for (let attempt = 0; attempt < 10 && document.activeElement.id !== "review-close"; attempt++) {
            await new Promise(resolve => setTimeout(resolve, 20));
          }
          return document.activeElement.id === "review-close";
        }), "Reopening Review moves focus to its panel control");
      ` : `
        await page.wait("#documents-toggle[aria-expanded='false']");
        await page.wait("#review-toggle[aria-expanded='false']");
        await page.click("#documents-toggle");
        await page.wait("#sidebar-backdrop:not([hidden])");
        check(await page.eval(() => document.querySelector("#layout").dataset.reviewOpen === "false"
          && document.querySelector("#reader").inert), "Mobile opens one overlay drawer at a time");
        await page.press("Escape");
        check(await page.eval(() => document.querySelector("#documents-toggle").getAttribute("aria-expanded") === "false"
          && document.activeElement.id === "documents-toggle"), "Escape closes the drawer and restores focus");
      `}
      await showPanel("review");
      check(await page.eval(() => !document.querySelector("#workspace") && !document.querySelector("#source")),
        "The header shows only the TrueSpec brand");
      check(await page.eval(() => !document.querySelector(".app-header button")
        && document.querySelector("#documents-close").closest("#documents-panel")
        && document.querySelector("#review-close").closest("#review")),
        "Panel controls stay attached to their sidebars instead of the app header");
      check(await page.eval(() => !document.querySelector("#workspace") && !document.querySelector("#source")),
        "The header should contain no workspace or source labels");
      check(await page.eval(() => {
        const prompt = document.querySelector("#agent-prompt").textContent;
        const help = document.querySelector("#agent-help");
        return help.matches("details") && !help.hidden && !help.open && prompt.includes(${JSON.stringify(viewer.review)})
          && !prompt.includes("truespec poll") && document.querySelector("#copy-prompt").textContent === "Copy prompt";
      }), "Standalone reviews show a collapsed, copyable agent prompt instead of a CLI command");
      await page.click("#agent-help > summary");
      check(await page.eval(() => document.querySelector("#agent-help").open), "The agent prompt disclosure expands");
      ${mode === "openspec" ? `
        check(await page.eval(() => {
          const group = [...document.querySelectorAll(".doc-group")].find(item => item.querySelector("h2")?.textContent === "change / revise-login");
          const regular = [...group.children].filter(item => item.matches("a.doc-link")).map(item => item.textContent);
          const folder = group.querySelector(".delta-group");
          return JSON.stringify(regular) === JSON.stringify(["design.md", "proposal.md", "tasks.md"])
            && folder.open && folder.querySelector("summary").textContent === "delta/"
            && [...folder.querySelectorAll(".doc-link")].map(item => item.textContent).join("|") === "account"
            && !group.textContent.includes("Delta:");
        }), "Active change documents are grouped under lowercase change and delta/");
      ` : ""}
      check(await page.eval(() => {
        const buttons = [...document.querySelectorAll("#handoff button")];
        return buttons.map(button => button.textContent).join("|") === "Send|Send and End" && buttons[0].disabled && !buttons[1].disabled;
      }), "The review exposes only Send and Send and End, and an empty review can end");
      check(await page.eval(() => !document.querySelector(".review-tabs")
        && document.querySelector(".review-empty h3").textContent === "No comments yet"),
        "Review starts as one unified feed without tabs");
      check(await page.eval(() => document.querySelector("#annotation-toggle").getAttribute("aria-checked") === "true"),
        "Annotation starts enabled");
      ${width < 960 ? `await hidePanel("review");` : ""}
      await page.click("#annotation-toggle");
      check(await page.eval(() => document.querySelector("#annotation-toggle").getAttribute("aria-checked") === "false"
        && document.querySelector("#comment-selection").disabled), "Turning annotation off disables new targets");
      await page.eval(() => {
        const word = document.querySelector("#login + p strong [data-run]");
        const rect = word.getClientRects()[0];
        word.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, pointerType: "mouse",
          clientX: (rect.left + rect.right) / 2, clientY: (rect.top + rect.bottom) / 2 }));
      });
      await page.wait(100);
      check(await page.eval(() => document.querySelector("#line-outline").hidden), "Annotation off hides hover outlines");
      await page.click("#article a");
      await page.wait("#article:not([hidden]) #timing");
      await page.back();
      await page.wait("#article:not([hidden]) #login");
      await page.click("#annotation-toggle");
      check(await page.eval(() => document.querySelector("#annotation-toggle").getAttribute("aria-checked") === "true"),
        "Annotation can be enabled again after following a Markdown link");
      await page.eval(() => {
        const word = document.querySelector("#login + p strong [data-run]");
        const rect = word.getClientRects()[0];
        word.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, pointerType: "mouse",
          clientX: (rect.left + rect.right) / 2, clientY: (rect.top + rect.bottom) / 2 }));
      });
      await page.wait("#line-outline[data-line='7']");
      check(await page.eval(() => innerWidth > 500 || document.querySelectorAll("#line-outline .target-box").length > 1), "One source line must outline all its visual wraps");
      await clickLine("#login + p strong [data-run]");
      await page.wait("#line-composer:not([hidden])");
      check(await page.eval(() => {
        const composer = document.querySelector("#line-composer").getBoundingClientRect();
        const glyphs = [...document.querySelectorAll("#line-outline .target-box")];
        return glyphs.length && composer.top >= Math.max(...glyphs.map(rect => rect.getBoundingClientRect().bottom))
          && document.activeElement.id === "line-comment" && document.querySelector("#annotation-toggle").disabled;
      }), "Click opens a focused composer directly below the outlined line");
      await page.fill("#line-comment", "Define the exact timeout.");
      await page.press("Shift+Enter");
      await page.type("Include units.");
      check(await page.eval(() => document.querySelector("#shortcut-count").textContent === "0"), "Typing a newline must not queue the draft");
    `);
    if (screenshots) chrome(["screenshot", join(screenshots, `truespec-${mode}-line.png`)]);

    await writeFile(join(root, first), `A new preamble from an external edit.\n\n${source}`);
    script(`
      await page.wait("#doc-notice:not([hidden])");
      check(await page.eval(() => document.querySelector("#agent-help").open), "The expanded agent prompt survives refresh");
      check(await page.eval(() => document.querySelector("#line-comment").value === "Define the exact timeout.\\nInclude units."), "Refresh must preserve the open composer");
      await page.press("Enter");
      await page.wait("#line-error:not([hidden])");
      await clickLine("#login + p strong [data-run]");
      check(await page.eval(() => document.querySelector("#line-comment").value === "Define the exact timeout.\\nInclude units."), "Retargeting must keep the draft text");
      check(await page.eval(() => document.querySelector("#doc-notice").hidden), "Retargeting clears the stale-target notice");
      await page.press("Enter");
      await page.wait(".annotation p");
      check(await page.eval(() => document.querySelector("#line-composer").hidden && document.querySelector("#shortcut-count").textContent === "1"), "Enter queues and closes the inline composer");
      await margin("paragraph", "#login + p");
      await page.wait("#line-outline[data-kind='paragraph']");
      check(await page.eval(() => {
        const border = document.querySelector("#line-outline .target-box").getBoundingClientRect();
        const paragraph = document.querySelector("#login + p").getBoundingClientRect();
        return border.top <= paragraph.top && border.bottom >= paragraph.bottom;
      }), "The inner margin must outline the full paragraph");
      await margin("paragraph", "#login + p", true);
      await page.wait("#line-composer:not([hidden])");
      check(await page.eval(() => document.querySelector("#line-label").textContent === "Paragraph"), "The form names the paragraph scope");
      await page.fill("#line-comment", "Rewrite this paragraph.");
      await page.press("Enter");
      await page.wait("#review-list > .annotation:nth-child(2)");
      await margin("section", "#login");
      await page.wait("#line-outline[data-kind='section'][data-heading='login']");
      check(await page.eval(() => {
        const border = document.querySelector("#line-outline .target-box").getBoundingClientRect();
        const heading = document.querySelector("#login").getBoundingClientRect();
        const paragraph = document.querySelector("#login + p").getBoundingClientRect();
        return border.top <= heading.top && border.bottom >= paragraph.bottom;
      }), "The outer margin must outline the heading and its content");
      await margin("section", "#login", true);
      await page.wait("#line-composer:not([hidden])");
      check(await page.eval(() => document.querySelector("#line-label").textContent === "Section: Login"), "The form names the targeted section");
    `);
    if (screenshots) chrome(["screenshot", join(screenshots, `truespec-${mode}-section.png`)]);
    script(`
      await page.fill("#line-comment", "Give this section a clearer structure.");
      await page.press("Enter");
      await page.wait("#review-list > .annotation:nth-child(3)");
      await margin("list", "#next-topic ~ [data-list] ul li");
      await page.wait("#line-outline[data-kind='list'][data-list='1']");
      check(await page.eval(() => {
        const outline = document.querySelector("#line-outline");
        const boxes = outline.querySelectorAll(".target-box");
        const border = boxes[0].getBoundingClientRect();
        const list = document.querySelector("#article [data-list='1']");
        const wholeList = list.getBoundingClientRect();
        const title = list.previousElementSibling.getBoundingClientRect();
        return boxes.length === 1 && !outline.querySelector(".target-label")
          && border.top <= wholeList.top && border.bottom >= wholeList.bottom && border.top > title.bottom;
      }), "The inner margin outlines the entire list without a floating label, title, or nested-item-only target");
      await margin("list", "#next-topic ~ [data-list] ul li", true);
      await page.wait("#line-composer:not([hidden])");
      const placement = await page.eval(() => ({
        label: document.querySelector("#line-label").textContent,
        composerTop: document.querySelector("#line-composer").getBoundingClientRect().top,
        outlineBottom: document.querySelector("#line-outline .target-box").getBoundingClientRect().bottom,
      }));
      check(placement.label === "List" && placement.composerTop >= placement.outlineBottom,
        "The composer names the whole list and keeps its items visible: " + JSON.stringify(placement));
    `);
    if (screenshots) chrome(["screenshot", join(screenshots, `truespec-${mode}-list.png`)]);
    script(`
      await page.fill("#line-comment", "This applies to the whole list.");
      await page.press("Enter");
      await page.wait("#review-list > .annotation:nth-child(4)");
      await page.click("#article a");
      await page.wait("#article:not([hidden]) #timing");
      await page.eval(() => {
        const start = [...document.querySelectorAll("#article [data-run]")].find(span => span.textContent === "formatted").firstChild;
        const end = document.querySelector("#article code [data-run]").firstChild;
        const range = new Range();
        range.setStart(start, 0);
        range.setEnd(end, end.length);
        getSelection().removeAllRanges();
        getSelection().addRange(range);
      });
      await page.wait("#comment-selection:not(:disabled)");
      check(await page.eval(() => {
        const selection = getSelection().getRangeAt(0).getBoundingClientRect();
        const button = document.querySelector("#comment-selection").getBoundingClientRect();
        const mobile = matchMedia("(max-width: 43rem)").matches;
        return document.querySelector("#line-composer").hidden && !document.querySelector("#comment-selection").hidden
          && (mobile ? button.top >= selection.bottom : button.bottom <= selection.top)
          && button.left >= document.querySelector("#reader").getBoundingClientRect().left;
      }), "Selection action uses the available side without covering selected text");
      await page.click("#annotation-toggle");
      await page.wait(100);
      check(await page.eval(() => document.querySelector("#comment-selection").hidden),
        "Turning Annotate off hides the contextual selection action");
      await page.click("#annotation-toggle");
      await page.wait("#comment-selection:not([hidden]):not(:disabled)");
      await page.click("#comment-selection");
      await page.fill("#comment", "Use a concrete example here.");
      await page.click("#save-comment");
      await page.wait("#review-list > .annotation:nth-child(5)");
      const reviewState = await page.eval(() => {
        const items = [...document.querySelectorAll("#review-list > .annotation")];
        const paths = items.map(item => item.querySelector(".annotation-path").textContent);
        return {
          count: document.querySelector("#shortcut-count").textContent,
          paths,
          draft: items.map(item => item.classList.contains("draft")),
          statuses: items.map(item => item.querySelector(".annotation-status")?.textContent),
        };
      });
      check(reviewState.count === "5"
        && reviewState.draft.every(Boolean) && reviewState.statuses.every(status => !status?.includes("Draft"))
        && new Set(reviewState.paths).size === 2
        && reviewState.paths.slice(0, 4).every(path => path === reviewState.paths[0])
        && reviewState.paths[4] !== reviewState.paths[0],
        "Five chronological drafts retain inline document context: " + JSON.stringify(reviewState));
      await showPanel("review");
      check(await page.eval(() => {
        const input = document.querySelector("#general-comment").getBoundingClientRect();
        const actions = document.querySelector("#handoff").getBoundingClientRect();
        return input.bottom <= actions.top;
      }), "General feedback input sits directly above the send actions");
      await page.fill("#general-comment", "A general note.");
      await page.press("Shift+Enter");
      await page.type("No annotation needed.");
      check(await page.eval(() => document.querySelector("#shortcut-count").textContent === "5"),
        "Shift+Enter does not queue general feedback");
      await page.press("Enter");
      await page.wait("#review-list > .annotation:nth-child(6)");
      check(await page.eval(() => {
        const item = document.querySelector("#review-list > .annotation:nth-child(6)");
        return document.activeElement.id === "general-comment" && document.querySelector("#general-comment").value === ""
          && !item.querySelector(".annotation-path") && !item.querySelector(".target-link")
          && item.querySelector(":scope > p").textContent === "A general note.\\nNo annotation needed."
          && !item.textContent.includes("Draft") && document.querySelector("#status").textContent === "Comment queued.";
      }), "Enter queues unanchored general feedback into the feed without a Draft label");
      check(await page.eval(() => {
        for (const context of document.querySelectorAll(".original-context")) context.open = true;
        const feed = document.querySelector("#review-scroll");
        feed.scrollTop = feed.scrollHeight;
        return feed.scrollHeight > feed.clientHeight && feed.scrollTop > 0
          && document.querySelector("#general-composer").getBoundingClientRect().bottom
            <= document.querySelector("#review").getBoundingClientRect().bottom;
      }), "Expanded original context scrolls inside Review without pushing its composer offscreen");
      check(await page.eval(() => document.documentElement.scrollWidth <= innerWidth + 1), "Content must fit the viewport");
      ${width < 960 ? `await hidePanel("review");` : ""}
      await page.click("#annotation-toggle");
      await showPanel("review");
      await page.click("#review-list > .annotation:nth-child(1) .target-link");
      await page.wait("#line-outline[data-line='9']");
      check(await page.eval(() => document.querySelector("#annotation-toggle").getAttribute("aria-checked") === "false"),
        "Existing annotation jumps remain available while new annotation is off");
      await page.click("#annotation-toggle");
      await showPanel("review");
      await page.click("#review-list > .annotation:nth-child(4) .target-link");
      await page.wait("#line-outline[data-kind='list']");
      await showPanel("review");
      await page.click("#review-list > .annotation:nth-child(5) .target-link");
      await page.wait("#article:not([hidden]) #timing");
      check(await page.eval(async () => {
        for (let frame = 0; frame < 120 && !CSS.highlights.has("truespec-target"); frame++) {
          await new Promise(requestAnimationFrame);
        }
        return CSS.highlights.has("truespec-target");
      }), "Selection jump must highlight the selected text");
      await page.eval(() => document.querySelector("#article").focus({ preventScroll: true }));
      await page.press("ArrowDown");
      await page.press("ArrowRight");
      await page.wait("#line-outline[data-kind]:not([data-kind='line'])");
      await page.press("Enter");
      await page.wait("#line-composer:not([hidden])");
      await page.fill("#line-comment", "This should not be queued.");
      await page.press("Escape");
      check(await page.eval(() => document.querySelector("#line-composer").hidden && document.querySelector("#shortcut-count").textContent === "6"), "Escape cancels the composer");
      await page.press("ArrowDown");
      await page.press("Enter");
      await page.wait("#line-composer:not([hidden])");
      await page.fill("#line-comment", "Temporary draft.");
      await page.press("Enter");
      await page.wait("#review-list > .annotation:nth-child(7)");
      await showPanel("review");
      await page.click("#review-list > .annotation:nth-child(7) .annotation-actions button:last-child");
      await page.wait("#confirmation[open]");
      await page.click("#confirm-action");
      await page.wait("#send-end:not(:disabled)");
      check(await page.eval(() => document.querySelector("#shortcut-count").textContent === "6"), "Remove should delete only the chosen draft");
    `);
    if (screenshots) chrome(["screenshot", join(screenshots, `truespec-${mode}-drafts.png`)]);

    const previous = viewer.review;
    await stop(viewer);
    viewer = await start(root, mode);
    assert.equal(viewer.review, previous);
    script(`
      await page.open(${JSON.stringify(viewer.url)});
      await showPanel("review");
      await page.wait("#review-list > .annotation:nth-child(6)");
      check(await page.eval(() => document.querySelector("#shortcut-count").textContent === "6"), "Drafts persist after restarting");
      await page.click("#review-list > .annotation:nth-child(5) .annotation-actions button:first-child");
      await page.fill("#comment", "Use a different, concrete example here.");
      await page.click("#save-comment");
      await page.wait("#send:not(:disabled)");
      await page.eval(() => {
        window.__reviewOrder = [...document.querySelectorAll("#review-list > .annotation")].map(item => item.dataset.commentId);
      });
      ${mode === "markdown" ? `
        await page.click("#send");
        await page.wait("#status:not([hidden])");
        check(await page.eval(() => JSON.stringify([...document.querySelectorAll("#review-list > .annotation")].map(item => item.dataset.commentId))
          === JSON.stringify(window.__reviewOrder)
          && [...document.querySelectorAll("#review-list > .annotation")].every(item => !item.classList.contains("draft"))
          && document.querySelector("#ended").hidden && document.querySelector("#send").disabled),
          "Send updates items in place and leaves the same unified review open");
        await hidePanel("review");
        await page.eval(() => document.querySelector("#article").focus({ preventScroll: true }));
        await page.press("ArrowDown");
        await page.press("Enter");
        await page.wait("#line-composer:not([hidden])");
        await page.fill("#line-comment", "A follow-up draft.");
        await page.press("Enter");
        await page.wait("#review-list > .annotation:nth-child(7)");
        check(await page.eval(() => JSON.stringify([...document.querySelectorAll("#review-list > .annotation")].slice(0, 6).map(item => item.dataset.commentId))
          === JSON.stringify(window.__reviewOrder)
          && document.querySelector("#review-list > .annotation:nth-child(7)").classList.contains("draft")
          && !document.querySelector("#review-list > .annotation:nth-child(7)").textContent.includes("Draft")),
          "A later draft is appended after submitted items");
      ` : ""}
      await showPanel("review");
      await page.wait("#send-end:not(:disabled)");
      await page.click("#send-end");
      await page.wait("#ended:not([hidden])");
      check(await page.eval(() => document.querySelector("#ended h3").textContent === "Return to your agent"), "Send and End should return the user to their agent");
      await clickLine("#article #login + p strong [data-run]");
      check(await page.eval(() => document.querySelector("#handoff").hidden && document.querySelector("#line-composer").hidden), "Ended reviews must be read-only");
      check(await page.eval(() => document.querySelectorAll("#review-list > .annotation").length === ${mode === "markdown" ? 7 : 6}),
        "All sent items remain in the unified feed");
      check(await page.eval(() => [...document.querySelectorAll("#review-list > .annotation")].every(item =>
        item.querySelector(".annotation-status").textContent.includes("Queued"))), "Display the disconnected queue accurately");
    `);
    await stop(viewer);

    const run = (...args) => call(cli, args, { cwd: root, env: { ...process.env, PATH: "" } });
    const poll = () => JSON.parse(run("poll", "--session", previous, "--json", "--wait", "0"));
    const batch = poll();
    assert.equal(batch.session_ended, true);
    assert.equal(batch.event, "batch");
    assert.equal(batch.items[0].source.path, first);
    assert.equal(batch.items[0].source.line, 9);
    assert.equal(batch.items[0].body, "Define the exact timeout.\nInclude units.");
    assert.equal(batch.items[1].original.original.kind, "paragraph");
    assert.match(batch.items[1].original.quote, /This second authored line belongs to the same paragraph/);
    assert.equal(batch.items[2].original.original.kind, "section");
    assert.equal(batch.items[2].original.original.heading, "login");
    const section = batch.items[2].source.source_range;
    const raw = Buffer.from(`A new preamble from an external edit.\n\n${source}`);
    assert.match(raw.subarray(section.start, section.end).toString(), /^## Login[\s\S]*Read the session policy/);
    assert.doesNotMatch(raw.subarray(section.start, section.end).toString(), /Next topic/);
    assert.equal(batch.items[3].original.original.kind, "list");
    assert.equal(batch.items[3].original.quote, "First goal.\nNested goal.\nLast goal.");
    assert.equal(batch.items[3].body, "This applies to the whole list.");
    const list = batch.items[3].source.source_range;
    const listedSource = raw.subarray(list.start, list.end).toString().trimEnd();
    assert.equal(listedSource, mode === "openspec"
      ? "- First goal.\n  - Nested goal.\n- Last goal."
      : "3. First goal.\n   - Nested goal.\n4. Last goal.");
    assert.equal(batch.items[4].source.path, second);
    assert.equal(batch.items[4].original.quote, "formatted 😀 token");
    assert.equal(batch.items[4].body, "Use a different, concrete example here.");
    assert.equal(batch.items[5].original.original.kind, "general");
    assert.equal(batch.items[5].source.path, null);
    assert.equal(batch.items[5].body, "A general note.\nNo annotation needed.");
    run("ack", batch.batch_id, "--lease", batch.lease);
    for (const item of batch.items) run("complete", batch.batch_id, item.comment_id, "--lease", batch.lease);
    if (mode === "markdown") {
      const followUp = poll();
      assert.equal(followUp.event, "batch");
      assert.equal(followUp.items.length, 1);
      assert.equal(followUp.items[0].body, "A follow-up draft.");
      run("ack", followUp.batch_id, "--lease", followUp.lease);
      run("complete", followUp.batch_id, followUp.items[0].comment_id, "--lease", followUp.lease);
    }
    assert.equal(poll().event, "ended");

    viewer = await start(root, mode);
    script(`
      await page.open(${JSON.stringify(viewer.url)});
      await showPanel("review");
      await page.wait("#ended:not([hidden])");
      check(await page.eval(() => [...document.querySelectorAll(".annotation-status")].length === ${mode === "markdown" ? 7 : 6} &&
        [...document.querySelectorAll(".annotation-status")].every(item => item.textContent.includes("Agent completed")
          && item.textContent.includes("Unresolved"))), "Completion should never resolve comments");
    `);
    console.log(`Verified ${mode} browser-to-agent handoff at ${width}px.`);
  } finally {
    await stop(viewer);
    await rm(root, { recursive: true, force: true });
  }
}

async function verifyMirroredReply() {
  const root = await mkdtemp(join(tmpdir(), "truespec-reply-"));
  let viewer;
  try {
    await fixture(root, "openspec");
    viewer = await start(root, "openspec");
    const run = (...args) => call(cli, args, { cwd: root, env: { ...process.env, PATH: "" } });
    const binding = JSON.parse(run(
      "connector", "attach", "--source", "openspec", "--kind", "browser-test",
      "--destination", "browser-chat", "--label", "Browser Test",
    ));
    const [document] = await viewerApi(viewer, "/api/documents");
    const page = await viewerApi(viewer, `/api/documents/${document.id}`);
    const comment = await viewerApi(viewer, `/api/reviews/${viewer.review}/comments`, {
      document_id: document.id,
      revision: page.revision,
      body: "Mirror the agent response.",
      target: { kind: "line", line: 1 },
    }, 201);
    const batch = await viewerApi(viewer, `/api/reviews/${viewer.review}/submit`, {
      key: randomUUID(),
      drafts: [{ id: comment.id, version: 1 }],
      end: false,
    });

    chrome(["open", viewer.url]);
    chrome(["emulate", "--viewport", "1200x800x1"]);
    script(`
      await page.wait("#review-list > .annotation");
      await page.click("#review-close");
      await page.wait("#review-toggle[aria-expanded='false']");
    `);
    const delivered = JSON.parse(run(
      "connector", "poll", "--connection", binding.connection_id, "--wait", "0",
    ));
    assert.equal(delivered.batch_id, batch.batch_id);
    run(
      "connector", "delivered", batch.batch_id, "--connection", binding.connection_id,
      "--lease", delivered.lease, "--receipt", "browser-agent-message",
    );
    await writeFile(
      join(root, "reply.md"),
      "## Updated\n\nApplied the **requested change**.\n\n<script>window.__unsafe = true</script>\n",
    );
    run(
      "connector", "respond", batch.batch_id, "--connection", binding.connection_id,
      "--reply-id", "browser-assistant-message", "--body-file", "reply.md",
    );
    script(`
      await page.wait("#review-toggle.is-unread");
      await page.click("#review-toggle");
      await page.wait(".agent-reply");
      check(await page.eval(() => {
        const reply = document.querySelector(".agent-reply");
        return !document.querySelector("#review-toggle").classList.contains("is-unread")
          && reply.querySelector("h2").textContent === "Updated"
          && reply.querySelector("strong").textContent === "requested change"
          && !reply.querySelector("script")
          && reply.querySelector(".reply-references summary").textContent === "Regarding 1 comment"
          && document.querySelector(".annotation-status").textContent.includes("Agent replied")
          && document.querySelector(".annotation-status").textContent.includes("Unresolved");
      }), "A safe, unread agent reply is mirrored once and linked without resolving its comment");
      await page.click(".reply-reference");
      await page.wait("#line-outline[data-line='1']");
    `);
    assert.equal((await viewerApi(viewer, `/api/reviews/${viewer.review}`)).replies.length, 1);
    console.log("Verified one mirrored agent reply with unread and comment linkage.");
  } finally {
    await stop(viewer);
    await rm(root, { recursive: true, force: true });
  }
}

try {
  await exercise("openspec", 1440);
  await exercise("markdown", 390);
  await verifyMirroredReply();
} finally {
  spawnSync("chrome-devtools-axi", ["stop"], { env: chromeEnv, stdio: "ignore", timeout: 10_000 });
}
