import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { once } from "node:events";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";

const cli = resolve(process.env.TRUESPEC_BIN || "target/debug/truespec");
const chromeEnv = {
  ...process.env,
  CHROME_DEVTOOLS_AXI_SESSION: `truespec-test-${process.pid}`,
  CHROME_DEVTOOLS_AXI_CHROME_ARGS: [
    process.env.CHROME_DEVTOOLS_AXI_CHROME_ARGS,
    "--force-prefers-reduced-motion",
  ].filter(Boolean).join(" "),
};
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
  await mkdir(join(root, "truespec/config"), { recursive: true });
  await writeFile(join(root, "truespec/config/project.json"), JSON.stringify({
    version: 1,
    source: {
      adapter: mode,
      path: mode === "openspec" ? "openspec" : "docs",
    },
  }, null, 2) + "\n");
  const child = spawn(cli, ["connector", "--protocol", "1", "serve"], {
    cwd: root, env: { ...process.env, PATH: "" }, stdio: ["ignore", "pipe", "inherit"],
  });
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
  const first = mode === "openspec"
    ? "openspec/specs/delivery/flux-reconciliation/spec.md"
    : "docs/delivery/flux-reconciliation.md";
  const second = mode === "openspec"
    ? "openspec/changes/revise-login/design.md"
    : "docs/delivery/image-and-release.md";
  const link = mode === "openspec"
    ? "../../../changes/revise-login/design.md"
    : "image-and-release.md";
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
    await mkdir(join(root, "openspec/specs/delivery/image-and-release"), { recursive: true });
    await mkdir(join(root, "openspec/specs/infrastructure/k3s-clusters"), { recursive: true });
    await writeFile(join(root, "openspec/specs/delivery/image-and-release/spec.md"), "# Image and release\n");
    await writeFile(join(root, "openspec/specs/infrastructure/k3s-clusters/spec.md"), "# K3s clusters\n");
    await writeFile(join(root, "openspec/changes/revise-login/proposal.md"), "# Proposal\n");
    await writeFile(join(root, "openspec/changes/revise-login/tasks.md"), "# Tasks\n");
    await mkdir(join(root, "openspec/changes/revise-login/specs/delivery/flux-reconciliation"), { recursive: true });
    await writeFile(
      join(root, "openspec/changes/revise-login/specs/delivery/flux-reconciliation/spec.md"),
      "# Flux reconciliation delta\n",
    );
    await mkdir(join(root, "openspec/changes/archive/older-one"), { recursive: true });
    await mkdir(join(root, "openspec/changes/archive/older-two"), { recursive: true });
    await writeFile(join(root, "openspec/changes/archive/older-one/proposal.md"), "# Older proposal\n");
    await writeFile(join(root, "openspec/changes/archive/older-two/tasks.md"), "# Older tasks\n");
  } else {
    await mkdir(join(root, "docs/infrastructure"), { recursive: true });
    await mkdir(join(root, "docs/teams/shared"), { recursive: true });
    await mkdir(join(root, "docs/products/shared"), { recursive: true });
    await writeFile(join(root, "docs/infrastructure/k3s-clusters.md"), "# K3s clusters\n");
    await writeFile(join(root, "docs/teams/shared/one.md"), "# Team shared\n");
    await writeFile(join(root, "docs/products/shared/two.md"), "# Product shared\n");
  }
  return { first, second, source };
}

async function exercise(mode, width) {
  const root = await mkdtemp(join(tmpdir(), `truespec-${mode}-`));
  let viewer;
  try {
    const { first, second, source } = await fixture(root, mode);
    viewer = await start(root, mode);
    const run = (...args) => call(cli, args, { cwd: root, env: { ...process.env, PATH: "" } });
    const binding = JSON.parse(run(
      "connector", "attach", "--kind", "browser-test",
      "--destination", `browser-${mode}`, "--label", "Browser Test",
    ));
    chrome(["open", viewer.url]);
    if (width < 500) chrome(["emulate", "--viewport", `${width}x880x1,mobile`]);
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
      check(await page.eval(() => matchMedia("(prefers-reduced-motion: reduce)").matches
        && getComputedStyle(document.querySelector(".switch-track")).transitionDuration !== "0.01ms"),
        "Normal switch motion remains enabled under reduced-motion preference");
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
        check(await page.eval(() => parseFloat(getComputedStyle(document.querySelector("#review")).transitionDuration) > 0.01),
          "Normal drawer motion remains enabled under reduced-motion preference");
        check(await page.eval(() => document.querySelector("#layout").dataset.reviewOpen === "false"
          && document.querySelector("#reader").inert), "Mobile opens one overlay drawer at a time");
        await page.press("Escape");
        check(await page.eval(() => document.querySelector("#documents-toggle").getAttribute("aria-expanded") === "false"
          && document.activeElement.id === "documents-toggle"), "Escape closes the drawer and restores focus");
      `}
      await showPanel("review");
      const reviewBeforeExpand = await page.eval(() => {
        const review = document.querySelector("#review").getBoundingClientRect();
        return {
          left: review.left,
          right: review.right,
          width: review.width,
          documentsOpen: document.querySelector("#layout").dataset.documentsOpen,
          reviewOpen: document.querySelector("#layout").dataset.reviewOpen,
        };
      });
      check(await page.eval(() => document.querySelector("#review-expand").nextElementSibling.id === "review-heading"
        && document.querySelector("#review-expand").getAttribute("aria-label") === "Expand Review"
        && document.querySelector("#review-expand").getAttribute("aria-pressed") === "false"),
        "The accessible expand control sits immediately before Review");
      await page.click("#review-expand");
      await page.wait("#layout[data-review-expanded='true']");
      const expandedReviewState = await page.eval(() => {
        const review = document.querySelector("#review").getBoundingClientRect();
        return {
          label: document.querySelector("#review-expand").getAttribute("aria-label"),
          pressed: document.querySelector("#review-expand").getAttribute("aria-pressed"),
          documentsInert: document.querySelector("#documents-panel").inert,
          readerInert: document.querySelector("#reader").inert,
          backdropHidden: document.querySelector("#sidebar-backdrop").hidden,
          headerHeight: document.querySelector(".review-head").getBoundingClientRect().height,
          left: review.left,
          right: review.right,
          viewport: innerWidth,
        };
      });
      check(expandedReviewState.label === "Restore Review"
        && expandedReviewState.pressed === "true"
        && expandedReviewState.documentsInert
        && expandedReviewState.readerInert
        && expandedReviewState.backdropHidden
        && expandedReviewState.headerHeight <= 48
        && expandedReviewState.left <= 1
        && expandedReviewState.right >= expandedReviewState.viewport - 1,
        "Expanded Review fills the workspace and removes neighboring surfaces from interaction: "
          + JSON.stringify(expandedReviewState));
      await page.press("Escape");
      await page.wait("#layout[data-review-expanded='false']");
      check(await page.eval(() => document.activeElement.id === "review-expand"
        && document.querySelector("#review-expand").getAttribute("aria-label") === "Expand Review"
        && !document.querySelector("#review").inert), "Escape restores Review and returns focus to its control");
      const reviewAfterRestore = await page.eval(() => {
        const review = document.querySelector("#review").getBoundingClientRect();
        return {
          left: review.left,
          right: review.right,
          width: review.width,
          documentsOpen: document.querySelector("#layout").dataset.documentsOpen,
          reviewOpen: document.querySelector("#layout").dataset.reviewOpen,
        };
      });
      check(Math.abs(reviewAfterRestore.left - reviewBeforeExpand.left) <= 1
        && Math.abs(reviewAfterRestore.right - reviewBeforeExpand.right) <= 1
        && Math.abs(reviewAfterRestore.width - reviewBeforeExpand.width) <= 1
        && reviewAfterRestore.documentsOpen === reviewBeforeExpand.documentsOpen
        && reviewAfterRestore.reviewOpen === reviewBeforeExpand.reviewOpen,
        "Restoring Review returns the exact prior panel geometry: "
          + JSON.stringify({ reviewBeforeExpand, reviewAfterRestore }));
      ${width >= 960 ? `
        check(await page.eval(() => {
          const handle = document.querySelector("#review-resizer");
          const minimum = Number(handle.getAttribute("aria-valuemin"));
          const maximum = Number(handle.getAttribute("aria-valuemax"));
          const current = Number(handle.getAttribute("aria-valuenow"));
          return !handle.hidden && handle.tabIndex === 0 && handle.getAttribute("role") === "separator"
            && handle.getAttribute("aria-orientation") === "vertical"
            && handle.getAttribute("aria-controls") === "review"
            && minimum < current && current < maximum;
        }), "Desktop Review exposes an enabled semantic resize separator");
        const pointerWidthBefore = await page.eval(() => document.querySelector("#review").getBoundingClientRect().width);
        await page.eval(() => {
          const handle = document.querySelector("#review-resizer");
          const left = document.querySelector("#review").getBoundingClientRect().left;
          const event = options => new PointerEvent(options.type, {
            bubbles: true, pointerId: 41, pointerType: "mouse", button: 0, buttons: options.buttons,
            clientX: options.clientX, clientY: 200,
          });
          handle.dispatchEvent(event({ type: "pointerdown", buttons: 1, clientX: left }));
          handle.dispatchEvent(event({ type: "pointermove", buttons: 1, clientX: left - 64 }));
          handle.dispatchEvent(event({ type: "pointerup", buttons: 0, clientX: left - 64 }));
        });
        const pointerWidthAfter = await page.eval(() => document.querySelector("#review").getBoundingClientRect().width);
        check(pointerWidthAfter >= pointerWidthBefore + 60
          && await page.eval(() => Number(localStorage.getItem("truespec.sidebar.review.width")) > 0
            && document.querySelector("#layout").dataset.reviewResizing === "false"),
          "Pointer drag widens Review and persists the chosen width");
        await page.eval(() => document.querySelector("#review-resizer").focus());
        const keyboardWidthBefore = await page.eval(() => document.querySelector("#review").getBoundingClientRect().width);
        await page.press("ArrowRight");
        const keyboardWidthNarrower = await page.eval(() => document.querySelector("#review").getBoundingClientRect().width);
        await page.press("ArrowLeft");
        const keyboardWidthRestored = await page.eval(() => document.querySelector("#review").getBoundingClientRect().width);
        check(keyboardWidthNarrower <= keyboardWidthBefore - 15
          && Math.abs(keyboardWidthRestored - keyboardWidthBefore) <= 1,
          "Keyboard arrows resize Review in the expected direction");
        await page.eval(() => {
          const handle = document.querySelector("#review-resizer");
          const event = options => new PointerEvent(options.type, {
            bubbles: true, pointerId: 42, pointerType: "mouse", button: 0, buttons: options.buttons,
            clientX: options.clientX, clientY: 200,
          });
          const left = document.querySelector("#review").getBoundingClientRect().left;
          handle.dispatchEvent(event({ type: "pointerdown", buttons: 1, clientX: left }));
          handle.dispatchEvent(event({ type: "pointermove", buttons: 1, clientX: 0 }));
          handle.dispatchEvent(event({ type: "pointerup", buttons: 0, clientX: 0 }));
          window.__savedReviewWidth = Number(localStorage.getItem("truespec.sidebar.review.width"));
        });
        const reviewUrl = await page.eval(() => location.href);
        await page.open(reviewUrl);
        await page.wait("#article:not([hidden]) #login");
        await showPanel("review");
        const persistedReviewWidth = await page.eval(() => ({
          applied: document.querySelector("#review").getBoundingClientRect().width,
          stored: Number(localStorage.getItem("truespec.sidebar.review.width")),
        }));
        check(persistedReviewWidth.stored > 0
          && Math.abs(persistedReviewWidth.applied - persistedReviewWidth.stored) <= 1,
          "The desktop Review width persists across reloads: " + JSON.stringify(persistedReviewWidth));
        await page.eval(() => document.querySelector("#review-resizer").focus());
        await page.press("Home");
        check(await page.eval(() => localStorage.getItem("truespec.sidebar.review.width") === null
          && Number(document.querySelector("#review-resizer").getAttribute("aria-valuenow"))
            === Math.round(document.querySelector("#review").getBoundingClientRect().width)),
          "Home resets Review to its responsive default and clears the saved preference");
        await page.eval(() => {
          const handle = document.querySelector("#review-resizer");
          const event = options => new PointerEvent(options.type, {
            bubbles: true, pointerId: 43, pointerType: "mouse", button: 0, buttons: options.buttons,
            clientX: options.clientX, clientY: 200,
          });
          const left = document.querySelector("#review").getBoundingClientRect().left;
          handle.dispatchEvent(event({ type: "pointerdown", buttons: 1, clientX: left }));
          handle.dispatchEvent(event({ type: "pointermove", buttons: 1, clientX: 0 }));
          handle.dispatchEvent(event({ type: "pointerup", buttons: 0, clientX: 0 }));
          window.__savedReviewWidth = Number(localStorage.getItem("truespec.sidebar.review.width"));
        });
      ` : `
        check(await page.eval(() => document.querySelector("#review-resizer").hidden
          && document.querySelector("#review-resizer").tabIndex === -1),
          "The resize separator is unavailable in a narrow drawer");
      `}
      check(await page.eval(() => !document.querySelector("#workspace") && !document.querySelector("#source")),
        "The header shows only the TrueSpec brand");
      check(await page.eval(() => !document.querySelector(".app-header button")
        && document.querySelector("#documents-close").closest("#documents-panel")
        && document.querySelector("#review-close").closest("#review")),
        "Panel controls stay attached to their sidebars instead of the app header");
      check(await page.eval(() => !document.querySelector("#workspace") && !document.querySelector("#source")),
        "The header should contain no workspace or source labels");
      check(await page.eval(() => !document.querySelector("#agent-help") && !document.querySelector("#connection")),
        "Initialized reviews omit standalone handoff and redundant connection labels");
      ${mode === "openspec" ? `
        const disclosureDefaults = await page.eval(() => {
          const current = document.querySelector("[data-disclosure-key='openspec:current']");
          const changes = document.querySelector("[data-disclosure-key='openspec:changes']");
          const group = changes.querySelector("[data-disclosure-key='openspec:change:revise-login']");
          const archiveRoot = document.querySelector("[data-disclosure-key='openspec:archives']");
          const archives = [...archiveRoot.querySelectorAll(
            ":scope > [data-disclosure-key^='openspec:archive:']",
          )];
          const regular = [...group.children].filter(item => item.matches("a.doc-link")).map(item => item.textContent);
          const delta = group.querySelector("[data-disclosure-key='openspec:change:revise-login/delta']");
          const deltaDelivery = delta.querySelector(
            "[data-disclosure-key='openspec:change:revise-login/delta/delivery']");
          const currentDelivery = current.querySelector(
            "[data-disclosure-key='openspec:current/delivery']");
          const currentInfrastructure = current.querySelector(
            "[data-disclosure-key='openspec:current/infrastructure']");
          return {
            topLevelLabels: [...document.querySelectorAll("#documents > .doc-group > summary .disclosure-label")]
              .map(item => item.textContent),
            currentOpen: current?.open,
            changesOpen: changes?.open,
            changesLabel: changes?.querySelector(":scope > summary .disclosure-label")?.textContent,
            changeOpen: group?.open,
            changeLabel: group?.querySelector(":scope > summary .disclosure-label")?.textContent,
            archiveRootOpen: archiveRoot?.open,
            archiveRootLabel: archiveRoot?.querySelector(":scope > summary .disclosure-label")?.textContent,
            archives: archives.map(item => ({
              label: item.querySelector(":scope > summary .disclosure-label")?.textContent,
              open: item.open,
            })),
            regular,
            deltaOpen: delta?.open,
            deltaLabel: delta?.querySelector(":scope > summary .disclosure-label")?.textContent,
            deltaDeliveryLabel: deltaDelivery?.querySelector(":scope > summary .disclosure-label")?.textContent,
            deltas: [...deltaDelivery.querySelectorAll(":scope > .doc-link")].map(item => item.textContent),
            currentDelivery: [...currentDelivery.querySelectorAll(":scope > .doc-link")].map(item => item.textContent),
            currentInfrastructure: [...currentInfrastructure.querySelectorAll(":scope > .doc-link")].map(item => item.textContent),
            folderLabels: [...document.querySelectorAll("summary .disclosure-label")]
              .map(item => item.textContent),
            hasLegacyDelta: group.textContent.includes("Delta:"),
          };
        });
        check(disclosureDefaults.topLevelLabels.join("|") === "Current specifications|Changes|Archives"
          && disclosureDefaults.currentOpen
          && disclosureDefaults.changesOpen && disclosureDefaults.changesLabel === "Changes"
          && disclosureDefaults.changeOpen && disclosureDefaults.changeLabel === "revise-login"
          && disclosureDefaults.archiveRootOpen && disclosureDefaults.archiveRootLabel === "Archives"
          && disclosureDefaults.archives.length
          && disclosureDefaults.archives.every(item => !item.open)
          && disclosureDefaults.archives.map(item => item.label).join("|") === "older-one|older-two"
          && JSON.stringify(disclosureDefaults.regular) === JSON.stringify(["design.md", "proposal.md", "tasks.md"])
          && disclosureDefaults.deltaOpen && disclosureDefaults.deltaLabel === "delta"
          && disclosureDefaults.deltaDeliveryLabel === "delivery"
          && disclosureDefaults.deltas.join("|") === "flux-reconciliation"
          && disclosureDefaults.currentDelivery.join("|") === "flux-reconciliation|image-and-release"
          && disclosureDefaults.currentInfrastructure.join("|") === "k3s-clusters"
          && disclosureDefaults.folderLabels.every(label => !label.includes("/"))
          && !disclosureDefaults.hasLegacyDelta,
          "Adapters provide recursive OpenSpec hierarchy and separator-free labels: "
            + JSON.stringify(disclosureDefaults));
        await page.eval(() => {
          const changes = document.querySelector("[data-disclosure-key='openspec:changes']");
          changes.querySelector(":scope > summary").focus();
        });
        await page.press("Enter");
        check(await page.eval(() => {
          const changes = document.querySelector("[data-disclosure-key='openspec:changes']");
          return !changes.open && document.activeElement === changes.querySelector(":scope > summary");
        }), "Enter toggles a top-level disclosure without moving summary focus");
        await page.press("Space");
        check(await page.eval(() => {
          return document.querySelector("[data-disclosure-key='openspec:changes']").open;
        }), "Space reopens a top-level disclosure");
        await page.click("[data-disclosure-key='openspec:change:revise-login/delta'] > summary");
        await page.click("[data-disclosure-key='openspec:archive:older-one'] > summary");
        await page.click("[data-disclosure-key='openspec:changes'] > summary");
        const disclosureUrl = await page.eval(() => location.href);
        await page.open(disclosureUrl);
        await page.wait("#article:not([hidden]) #login");
        check(await page.eval(() => {
          const changes = document.querySelector("[data-disclosure-key='openspec:changes']");
          const group = changes.querySelector("[data-disclosure-key='openspec:change:revise-login']");
          const archiveRoot = document.querySelector("[data-disclosure-key='openspec:archives']");
          const openedArchive = document.querySelector(
            "[data-disclosure-key='openspec:archive:older-one']");
          const otherArchives = [...archiveRoot.querySelectorAll(
            ":scope > [data-disclosure-key^='openspec:archive:']",
          )].filter(item =>
            item.dataset.disclosureKey.startsWith("openspec:archive:")
              && item !== openedArchive);
          return !changes.open && group.open
            && !group.querySelector("[data-disclosure-key='openspec:change:revise-login/delta']").open
            && archiveRoot.open && openedArchive.open && otherArchives.every(item => !item.open);
        }), "Lifecycle, named-child, and nested disclosure states persist independently after reload");
        const deltaHref = await page.eval(() => {
          return document.querySelector(
            "[data-disclosure-key='openspec:change:revise-login/delta/delivery'] > .doc-link").href;
        });
        await page.open(deltaHref);
        await page.wait(".doc-link[aria-current='page']");
        check(await page.eval(() => {
          const active = document.querySelector(".doc-link[aria-current='page']");
          return [...active.parentElement.closest(".doc-group").querySelectorAll("details")]
            .filter(folder => folder.contains(active)).every(folder => folder.open)
            && active.closest(".doc-group").open
            && localStorage.getItem("truespec.documents.disclosure:openspec:changes") === "true"
            && localStorage.getItem("truespec.documents.disclosure:openspec:change:revise-login") === "true"
            && localStorage.getItem("truespec.documents.disclosure:openspec:change:revise-login/delta") === "true"
            && localStorage.getItem("truespec.documents.disclosure:openspec:change:revise-login/delta/delivery") === "true";
        }), "A selected delta document opens and persists every ancestor disclosure");
        await page.eval(() => {
          const group = document.querySelector("[data-disclosure-key='openspec:change:revise-login']");
          group.open = false;
          group.dispatchEvent(new Event("toggle"));
          const href = [...group.querySelectorAll(":scope > .doc-link")]
            .find(link => link.textContent === "design.md").href;
          history.pushState(null, "", href);
          dispatchEvent(new PopStateEvent("popstate"));
        });
        await page.wait(".doc-link[aria-current='page']");
        check(await page.eval(() => {
          const active = document.querySelector(".doc-link[aria-current='page']");
          return active.textContent === "design.md"
            && active.closest("[data-disclosure-key='openspec:change:revise-login']").open
            && active.closest(".doc-group").open;
        }), "Browser-history navigation reveals a document inside a closed group");
        await page.open(${JSON.stringify(viewer.url)});
        await page.wait("#article:not([hidden]) #login");
        await page.click("[data-disclosure-key='openspec:change:revise-login/delta'] > summary");
      ` : `
        await showPanel("documents");
        check(await page.eval(() => {
          const root = document.querySelector("[data-disclosure-key='markdown:documents']");
          const delivery = document.querySelector("[data-disclosure-key='markdown:documents/delivery']");
          const infrastructure = document.querySelector("[data-disclosure-key='markdown:documents/infrastructure']");
          const teamShared = document.querySelector("[data-disclosure-key='markdown:documents/teams/shared']");
          const productShared = document.querySelector("[data-disclosure-key='markdown:documents/products/shared']");
          return root.open && delivery.open && infrastructure.open && teamShared.open && productShared.open
            && [...delivery.querySelectorAll(":scope > .doc-link")].map(link => link.textContent).join("|")
              === "flux-reconciliation.md|image-and-release.md"
            && [...infrastructure.querySelectorAll(":scope > .doc-link")].map(link => link.textContent).join("|")
              === "k3s-clusters.md"
            && teamShared.dataset.disclosureKey !== productShared.dataset.disclosureKey
            && [...document.querySelectorAll(".document-folder > summary .disclosure-label")]
              .every(label => !label.textContent.includes("/"));
        }), "Markdown adapters provide recursive folders and independent duplicate names in the mobile drawer");
        await page.eval(() => document.querySelector(".doc-group > summary").focus());
        await page.press("Enter");
        const mobileDisclosureState = await page.eval(() => ({
          open: document.querySelector(".doc-group").open,
          activeTag: document.activeElement.tagName,
          activeText: document.activeElement.textContent,
          focusedSummary: document.activeElement.matches(".doc-group > summary"),
        }));
        check(!mobileDisclosureState.open && mobileDisclosureState.focusedSummary,
          "Native disclosure keyboard behavior works inside the mobile drawer: "
            + JSON.stringify(mobileDisclosureState));
        await page.press("Enter");
        await showPanel("review");
      `}
      const compactControls = await page.eval(() => {
        const buttons = [...document.querySelectorAll("#handoff button")];
        const actions = document.querySelector("#handoff").getBoundingClientRect();
        const buttonRects = buttons.map(button => button.getBoundingClientRect());
        const toolbar = document.querySelector("#doc-toolbar").getBoundingClientRect();
        const reader = document.querySelector("#reader").getBoundingClientRect();
        return {
          labels: buttons.map(button => button.textContent).join("|"),
          disabled: buttons.map(button => button.disabled),
          actionWidth: actions.width,
          buttonWidths: buttonRects.map(rect => rect.width),
          buttonHeights: buttonRects.map(rect => rect.height),
          trailingGap: actions.right - buttonRects.at(-1).right,
          reviewHeaderHeight: document.querySelector(".review-head").getBoundingClientRect().height,
          toolbarHeight: toolbar.height,
          toolbarInset: toolbar.top - reader.top,
        };
      });
      check(compactControls.labels === "Send|Send and End"
        && compactControls.disabled[0] && !compactControls.disabled[1],
        "The review exposes only Send and Send and End, and an empty review can end");
      check(compactControls.buttonWidths.every(width => width < compactControls.actionWidth * 0.6)
        && compactControls.buttonHeights.every(height => height >= 32 && height <= 40)
        && Math.abs(compactControls.trailingGap) <= 1,
        "Send actions are compact, practical targets aligned to the footer edge: "
          + JSON.stringify(compactControls));
      check(compactControls.reviewHeaderHeight <= 48
        && compactControls.toolbarHeight <= 44
        && compactControls.toolbarInset <= 12,
        "Review and document control bars preserve content space: " + JSON.stringify(compactControls));
      const composerDisclosure = await page.eval(() => {
        const toggle = document.querySelector("#general-composer-toggle");
        const body = document.querySelector("#general-composer-body");
        return {
          expanded: toggle.getAttribute("aria-expanded"),
          label: toggle.getAttribute("aria-label"),
          bodyHidden: body.hidden,
          collapsed: document.querySelector("#general-composer").dataset.collapsed,
          opacity: Number.parseFloat(getComputedStyle(toggle).opacity),
        };
      });
      check(composerDisclosure.expanded === "true"
        && composerDisclosure.label === "Collapse general feedback"
        && !composerDisclosure.bodyHidden && composerDisclosure.collapsed === "false"
        && (${width < 960} ? composerDisclosure.opacity === 1 : composerDisclosure.opacity > 0),
        "General feedback starts expanded with an accessible disclosure control: "
          + JSON.stringify(composerDisclosure));
      await page.eval(() => document.querySelector("#general-composer-toggle").focus());
      check(await page.eval(() => document.activeElement.id === "general-composer-toggle"
        && Number.parseFloat(getComputedStyle(document.activeElement).opacity) === 1),
        "Keyboard focus reveals the composer disclosure control");
      await page.click("#general-composer-toggle");
      check(await page.eval(() => {
        const form = document.querySelector("#general-composer");
        const toggle = document.querySelector("#general-composer-toggle");
        return form.dataset.collapsed === "true"
          && document.querySelector("#general-composer-body").hidden
          && toggle.getAttribute("aria-expanded") === "false"
          && toggle.getAttribute("aria-label") === "Expand general feedback"
          && localStorage.getItem("truespec.general-composer.collapsed") === "true";
      }), "Collapsing general feedback hides its body and saves the preference");
      const collapsedComposerUrl = await page.eval(() => location.href);
      await page.open(collapsedComposerUrl);
      await page.wait("#article:not([hidden])");
      await showPanel("review");
      check(await page.eval(() => document.querySelector("#general-composer").dataset.collapsed === "true"
        && document.querySelector("#general-composer-body").hidden),
        "The collapsed general-feedback preference survives reload");
      await page.click("#general-composer-toggle");
      await page.fill("#general-comment", "Pending feedback stays here.");
      await page.click("#general-composer-toggle");
      check(await page.eval(() => {
        const form = document.querySelector("#general-composer");
        const toggle = document.querySelector("#general-composer-toggle");
        return form.dataset.collapsed === "true" && form.dataset.pending === "true"
          && document.querySelector("#general-comment").value === "Pending feedback stays here."
          && !document.querySelector("#send").disabled
          && toggle.getAttribute("aria-label") === "Expand general feedback, pending feedback";
      }), "Collapsed pending feedback stays visible to submission and exposes its hidden state");
      await page.click("#general-composer-toggle");
      check(await page.eval(() => document.activeElement.id === "general-comment"
        && document.querySelector("#general-comment").value === "Pending feedback stays here."),
        "Expanding general feedback restores the same text and focuses the textarea");
      await page.fill("#general-comment", "");
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
    if (width >= 960) {
      chrome(["resize", "1024", "880"]);
      script(`
        await page.wait(60);
        const clampedReview = await page.eval(() => {
          const handle = document.querySelector("#review-resizer");
          return {
            applied: document.querySelector("#review").getBoundingClientRect().width,
            stored: Number(localStorage.getItem("truespec.sidebar.review.width")),
            maximum: Number(handle.getAttribute("aria-valuemax")),
            reader: document.querySelector("#reader").getBoundingClientRect().width,
          };
        });
        check(clampedReview.applied <= clampedReview.maximum + 1
          && clampedReview.applied < clampedReview.stored
          && clampedReview.reader >= 320,
          "A narrower viewport clamps Review without discarding its saved preference: "
            + JSON.stringify(clampedReview));
      `);
      chrome(["resize", String(width), "880"]);
      script(`
        await page.wait(60);
        check(await page.eval(() => Math.abs(
          document.querySelector("#review").getBoundingClientRect().width
            - Number(localStorage.getItem("truespec.sidebar.review.width"))
        ) <= 1), "Review returns to its saved width when space becomes available");
      `);
    }

    await writeFile(join(root, first), `A new preamble from an external edit.\n\n${source}`);
    script(`
      await page.wait("#doc-notice:not([hidden])");
      ${mode === "openspec" ? `
        check(await page.eval(() => {
          const group = document.querySelector("[data-disclosure-key='openspec:change:revise-login']");
          return group.open
            && !group.querySelector("[data-disclosure-key='openspec:change:revise-login/delta']").open;
        }), "Source refresh preserves independent disclosure state");
      ` : ""}
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

    script(`
      await showPanel("review");
      await page.click("#review-expand");
      await page.wait("#layout[data-review-expanded='true']");
    `);
    if (screenshots) chrome(["screenshot", join(screenshots, `truespec-${mode}-expanded-review.png`)]);
    const previous = viewer.review;
    await stop(viewer);
    viewer = await start(root, mode);
    assert.equal(viewer.review, previous);
    script(`
      await page.open(${JSON.stringify(viewer.url)});
      await page.wait("#layout[data-review-expanded='false']");
      check(await page.eval(() => document.querySelector("#review-expand").getAttribute("aria-pressed") === "false"),
        "Expanded Review is temporary across reloads");
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
      check(await page.eval(() => [...document.querySelectorAll("#review-list > .annotation")].every(item => {
        const indicator = item.querySelector(".delivery-indicator.working");
        return indicator?.getAttribute("aria-label") === "Queued for Browser Test"
          && getComputedStyle(indicator.querySelector(".delivery-spinner")).animationName === "delivery-spin"
          && parseFloat(getComputedStyle(indicator.querySelector(".delivery-spinner")).animationDuration) > 0
          && !/Resolved|Unresolved/.test(item.textContent)
          && !item.querySelector(".annotation-actions");
      })), "Display a labeled working spinner while paired delivery is pending");
    `);
    if (mode === "openspec") {
      chrome(["emulate", "--viewport", "390x880x1,mobile"]);
      script(`
        await showPanel("documents");
        const phoneHierarchy = await page.eval(() => {
          const changes = document.querySelector("[data-disclosure-key='openspec:changes']");
          const change = document.querySelector("[data-disclosure-key='openspec:change:revise-login']");
          const archives = document.querySelector("[data-disclosure-key='openspec:archives']");
          return {
            width: document.querySelector("#documents-panel").getBoundingClientRect().width,
            changesOpen: changes.open,
            changeOpen: change.open,
            archivesOpen: archives.open,
            archivedChildrenClosed: [...archives.querySelectorAll(
              ":scope > [data-disclosure-key^='openspec:archive:']",
            )]
              .every(item => !item.open),
          };
        });
        check(phoneHierarchy.width <= 390
          && phoneHierarchy.changesOpen && phoneHierarchy.changeOpen
          && phoneHierarchy.archivesOpen && phoneHierarchy.archivedChildrenClosed,
          "Shared OpenSpec lifecycle hierarchy retains its defaults in the phone drawer: "
            + JSON.stringify(phoneHierarchy));
      `);
      if (screenshots) {
        chrome(["screenshot", join(screenshots, "truespec-openspec-phone-navigation.png")]);
      }
    }
    await stop(viewer);

    const poll = () => JSON.parse(run(
      "connector", "poll", "--connection", binding.connection_id, "--wait", "0",
    ));
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
    const finish = async (incoming, replyID) => {
      run(
        "connector", "delivered", incoming.batch_id, "--connection", binding.connection_id,
        "--lease", incoming.lease, "--receipt", `receipt-${replyID}`,
      );
      const reply = join(root, `reply-${replyID}.md`);
      await writeFile(reply, `Handled ${incoming.items.length} review comments.\\n`);
      run(
        "connector", "respond", incoming.batch_id, "--connection", binding.connection_id,
        "--reply-id", replyID, "--body-file", reply,
      );
    };
    await finish(batch, "first");
    if (mode === "markdown") {
      const followUp = poll();
      assert.equal(followUp.event, "batch");
      assert.equal(followUp.items.length, 1);
      assert.equal(followUp.items[0].body, "A follow-up draft.");
      await finish(followUp, "follow-up");
    }
    assert.equal(poll().event, "ended");

    viewer = await start(root, mode);
    script(`
      await page.open(${JSON.stringify(viewer.url)});
      await showPanel("review");
      await page.wait("#ended:not([hidden])");
      check(await page.eval(() => [...document.querySelectorAll(".annotation-status")].length === ${mode === "markdown" ? 7 : 6} &&
        [...document.querySelectorAll(".annotation-status")].every(item =>
          item.querySelector(".delivery-indicator.done")?.getAttribute("aria-label") === "Agent done"
          && !/Resolved|Unresolved/.test(item.textContent))
        && ![...document.querySelectorAll(".annotation:not(.draft) .annotation-actions")].length),
        "Handled feedback shows only agent progress without reviewer resolution UI");
    `);
    console.log(`Verified ${mode} browser-to-agent handoff at ${width}px.`);
  } finally {
    await stop(viewer);
    await rm(root, { recursive: true, force: true });
  }
}

async function verifyPendingGeneralSubmission() {
  const root = await mkdtemp(join(tmpdir(), "truespec-general-send-"));
  let viewer;
  try {
    await fixture(root, "openspec");
    viewer = await start(root, "openspec");
    const run = (...args) => call(cli, args, { cwd: root, env: { ...process.env, PATH: "" } });
    const binding = JSON.parse(run(
      "connector", "attach", "--kind", "browser-test",
      "--destination", "general-send-chat", "--label", "Browser Test",
    ));
    chrome(["open", viewer.url]);
    chrome(["emulate", "--viewport", "1200x800x1"]);
    script(`
      await showPanel("review");
      await page.fill("#general-comment", "Send directly.");
      await page.click("#general-composer-toggle");
      check(await page.eval(() => !document.querySelector("#send").disabled
        && document.querySelector("#shortcut-count").textContent === "0"
        && document.querySelector("#general-composer").dataset.collapsed === "true"),
        "Collapsed typed general feedback enables Send without Enter");
      await page.click("#send");
      await page.wait("#review-list > .annotation");
      check(await page.eval(() => document.querySelector("#general-comment").value === ""
        && document.querySelector("#review-list > .annotation p").textContent === "Send directly."
        && document.querySelector("#review-list > .annotation").classList.contains("draft") === false
        && document.querySelector("#general-composer").dataset.collapsed === "true"),
        "Send submits collapsed pending feedback directly and clears only after success");
      await page.click("#general-composer-toggle");
    `);
    const first = JSON.parse(run(
      "connector", "poll", "--connection", binding.connection_id, "--wait", "0",
    ));
    assert.equal(first.items.length, 1);
    assert.equal(first.items[0].body, "Send directly.");
    assert.equal(first.items[0].original.original.kind, "general");
    run(
      "connector", "delivered", first.batch_id, "--connection", binding.connection_id,
      "--lease", first.lease, "--receipt", "direct-general-message",
    );
    await writeFile(join(root, "direct-general-reply.md"), "Handled direct general feedback.\n");
    run(
      "connector", "respond", first.batch_id, "--connection", binding.connection_id,
      "--reply-id", "direct-general-reply", "--body-file", "direct-general-reply.md",
    );

    script(`
      await page.fill("#general-comment", "Queued first.");
      await page.press("Enter");
      await page.wait("#review-list > .annotation:nth-child(2)");
      await page.fill("#general-comment", "apply");
      await page.click("#general-composer-toggle");
      check(await page.eval(() => !document.querySelector("#send").disabled
        && !document.querySelector("#send-end").disabled
        && document.querySelector("#general-composer").dataset.collapsed === "true"),
        "Collapsed final text remains sendable alongside a queued draft");
      await page.eval(() => {
        const nativeFetch = window.fetch.bind(window);
        window.__nativeFetch = nativeFetch;
        window.__failedSubmitBeforeNetwork = false;
        window.fetch = async (...args) => {
          if (String(args[0]).endsWith("/submit")) {
            window.__lastSubmitArgs = args;
          }
          if (!window.__failedSubmitBeforeNetwork && String(args[0]).endsWith("/submit")) {
            window.__failedSubmitBeforeNetwork = true;
            throw new TypeError("Simulated network failure");
          }
          return nativeFetch(...args);
        };
      });
      await page.click("#send-end");
      await page.wait("#error:not([hidden])");
      check(await page.eval(() => document.querySelector("#general-comment").value === "apply"
        && !document.querySelector("#send-end").disabled
        && document.querySelector("#general-composer").dataset.collapsed === "true"),
        "A failed submit preserves collapsed final feedback for retry");
      await page.click("#send-end");
      await page.wait("#ended:not([hidden])");
      check(await page.eval(() => document.querySelector("#general-comment").value === ""
        && [...document.querySelectorAll("#review-list > .annotation p")]
          .filter(item => item.textContent === "apply").length === 1),
        "Retry submits final general feedback exactly once");
      check(await page.eval(async () => {
        const response = await window.__nativeFetch(...window.__lastSubmitArgs);
        return response.status === 200;
      }), "Repeating the exact successful submission replays idempotently");
      await page.wait(50);
      check(await page.eval(() => [...document.querySelectorAll("#review-list > .annotation p")]
        .filter(item => item.textContent === "apply").length === 1),
        "Idempotent replay does not duplicate final general feedback");
    `);
    const finalBatch = JSON.parse(run(
      "connector", "poll", "--connection", binding.connection_id, "--wait", "0",
    ));
    assert.equal(finalBatch.session_ended, true);
    assert.deepEqual(finalBatch.items.map(item => item.body), ["Queued first.", "apply"]);
    assert.equal(finalBatch.items[1].original.original.kind, "general");
    assert.equal(
      (await viewerApi(viewer, `/api/reviews/${viewer.review}`)).comments
        .filter(comment => comment.body === "apply").length,
      1,
    );
    console.log("Verified direct general Send and idempotent Send and End retry.");
  } finally {
    await stop(viewer);
    await rm(root, { recursive: true, force: true });
  }
}

async function verifyMirroredReply() {
  const root = await mkdtemp(join(tmpdir(), "truespec-reply-"));
  let viewer;
  try {
    const { first } = await fixture(root, "openspec");
    viewer = await start(root, "openspec");
    const run = (...args) => call(cli, args, { cwd: root, env: { ...process.env, PATH: "" } });
    const binding = JSON.parse(run(
      "connector", "attach", "--kind", "browser-test",
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
    await viewerApi(
      viewer,
      `/api/reviews/${viewer.review}/comments/${comment.id}`,
      { action: "resolve", resolved: true },
      204,
    );

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
          && document.querySelector(".annotation-status .delivery-indicator.done")?.getAttribute("aria-label") === "Agent done"
          && !/Resolved|Unresolved/.test(document.querySelector(".annotation-status").textContent)
          && !document.querySelector(".annotation:not(.draft) .annotation-actions");
      }), "A safe, unread agent reply is mirrored once with no reviewer resolution UI");
      await page.click(".reply-reference");
      await page.wait("#line-outline[data-line='1']");
    `);
    await writeFile(join(root, first), "# Changed document\n\nThe old target is gone.\n");
    script(`
      await page.wait(".annotation-status .status.warning");
      check(await page.eval(() => {
        const states = [...document.querySelector(".annotation-status").children];
        return states[0].classList.contains("delivery-indicator")
          && states[1].textContent === "Outdated"
          && states[1].title === "The document changed, so this comment may point to the wrong place."
          && states.length === 2;
      }), "Agent progress appears first and Outdated remains the only additional status");
    `);
    await rm(join(root, first));
    script(`
      await page.wait(2200);
      check(await page.eval(() => {
        const states = [...document.querySelector(".annotation-status").children];
        return states[0].classList.contains("delivery-indicator")
          && states[1].textContent === "Source missing"
          && states.length === 2
          && !document.querySelector(".annotation:not(.draft) .annotation-actions");
      }), "Source missing remains visible without reviewer resolution UI");
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
  await verifyPendingGeneralSubmission();
  await verifyMirroredReply();
} finally {
  spawnSync("chrome-devtools-axi", ["stop"], { env: chromeEnv, stdio: "ignore", timeout: 10_000 });
}
