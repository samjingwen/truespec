import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import { access, chmod, mkdir, mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { randomUUID } from "node:crypto";

const root = await realpath(await mkdtemp(join(tmpdir(), "truespec-opencode-")));
const workspace = join(root, "workspace");
const reviewedPath = "openspec/specs/reader/spec.md";
const binary = resolve(process.env.TRUESPEC_BIN || "target/debug/truespec");
const opencode = process.env.OPENCODE_BIN || "opencode";
const provider = createServer();
let server;
let log = "";
let providerError;
let serviceEnv;

async function until(description, check, timeout = 20_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (providerError) throw providerError;
    const value = await check();
    if (value) return value;
    await delay(100);
  }
  throw new Error(`Timed out waiting for ${description}.\n${log}`);
}

async function api(origin, path, body, expected = 200) {
  const workspace = new URL(origin).pathname.match(/^\/w\/([^/]+)\/$/)?.[1];
  const scoped = workspace && path.startsWith("/api/")
    ? `/api/workspaces/${workspace}${path.slice(4)}`
    : path;
  const response = await fetch(new URL(scoped, origin), body === undefined ? {} : {
    method: "POST",
    headers: { "Content-Type": "application/json", Origin: new URL(origin).origin },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, expected, `${path}: ${text}`);
  return text ? JSON.parse(text) : null;
}

function fakeModel(request, response) {
  void (async () => {
    if (request.url !== "/v1/chat/completions") {
      response.writeHead(404).end();
      return;
    }
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const payload = JSON.parse(Buffer.concat(chunks).toString());
    const messages = payload.messages || [];
    const userIndex = messages.findLastIndex(item => item.role === "user");
    const text = JSON.stringify(messages[userIndex]?.content || "");
    const round = text.includes("Use markdown revision.")
      ? "Markdown revision."
      : text.includes("Use revision two.")
        ? "Revision two."
        : text.includes("Use revision one.")
          ? "Revision one."
          : null;
    const model = payload.model;
    const replay = messages.slice(userIndex + 1).some(item => item.role === "tool");
    const tool = payload.tools?.find(item => item.function?.name?.endsWith("rewrite_fixture"))?.function?.name;
    let delta = { content: round ? `I updated the document. ${round}` : "Review fixture." };
    let finish = "stop";
    if (round && !replay) {
      assert.ok(tool, "The model request must expose the isolated Markdown edit tool");
      delta = { tool_calls: [{ index: 0, id: `call_${randomUUID()}`, type: "function",
        function: { name: tool, arguments: JSON.stringify({ revision: round }) } }] };
      finish = "tool_calls";
    }
    const chunk = (delta, finish_reason) => ({ id: "fixture-completion", object: "chat.completion.chunk", created: 1, model,
      choices: [{ index: 0, delta, finish_reason }] });
    response.writeHead(200, { "Content-Type": "text/event-stream" });
    response.write(`data: ${JSON.stringify(chunk({ role: "assistant", ...delta }, null))}\n\n`);
    response.write(`data: ${JSON.stringify(chunk({}, finish))}\n\n`);
    response.end("data: [DONE]\n\n");
  })().catch(error => {
    providerError = error;
    response.writeHead(500).end();
  });
}

try {
  provider.on("request", fakeModel);
  provider.listen(0, "127.0.0.1");
  await once(provider, "listening");
  const providerUrl = `http://127.0.0.1:${provider.address().port}/v1`;
  await mkdir(dirname(join(workspace, reviewedPath)), { recursive: true });
  await mkdir(join(workspace, ".git"));
  await mkdir(join(workspace, ".opencode/plugins"), { recursive: true });
  await writeFile(join(workspace, reviewedPath), "# Reader\n\nOriginal revision.\n");
  await writeFile(join(workspace, "opencode.json"), JSON.stringify({
    model: "fixture/mock",
    providers: {
      fixture: {
        package: "@opencode/ai/providers/openai-compatible",
        settings: { baseURL: providerUrl, apiKey: "test-only", timeout: 8_000 },
        compatibility: { streamUsage: false },
        models: { mock: { capabilities: { tools: true } } },
      },
    },
  }));
  await writeFile(join(workspace, ".opencode/plugins/rewrite-fixture.ts"), `
    import { Plugin } from "@opencode/plugin"
    import { writeFile } from "node:fs/promises"
    import { join } from "node:path"
    export default Plugin.define({
      id: "rewrite-fixture",
      async setup(ctx) {
        await ctx.tool.transform(editor => editor.add({
          name: "rewrite_fixture",
          description: "Update the deterministic Markdown fixture",
          input: { type: "object", properties: { revision: { enum: ["Revision one.", "Revision two."] } },
            required: ["revision"], additionalProperties: false },
          options: { codemode: false },
          async execute({ revision }) {
            await writeFile(join(ctx.location.directory, ${JSON.stringify(reviewedPath)}), "# Reader\\n\\n" + revision + "\\n")
            return { content: "Updated the fixture." }
          },
        }))
      },
    })
  `);

  const home = join(root, "home");
  await mkdir(home);
  const portProbe = createServer();
  portProbe.listen(0, "127.0.0.1");
  await once(portProbe, "listening");
  const servicePort = portProbe.address().port;
  portProbe.close();
  await once(portProbe, "close");
  const tailscaleState = join(root, "tailscale-state.json");
  const fakeTailscale = join(root, "tailscale");
  await writeFile(fakeTailscale, `#!/bin/sh
set -eu
if [ "$1 $2" = "status --json" ]; then
  printf '%s' '{"BackendState":"Running","Self":{"Online":true,"DNSName":"device.example.ts.net."}}'
elif [ "$1 $2 $3" = "serve status --json" ]; then
  if [ -f "${tailscaleState}" ]; then
    IFS= read -r value < "${tailscaleState}" || true
    printf '%s' "$value"
  else
    printf '{}'
  fi
elif [ "$1 $2" = "serve --bg" ]; then
  port="\${4#--https=}"
  target="$5"
  printf '{"TCP":{"%s":{"HTTPS":true}},"Web":{"device.example.ts.net:%s":{"Handlers":{"/":{"Proxy":"%s"}}}}}' "$port" "$port" "$target" > "${tailscaleState}"
elif [ "$1" = "serve" ] && [ "$3" = "off" ]; then
  printf '{}' > "${tailscaleState}"
else
  exit 9
fi
`);
  await chmod(fakeTailscale, 0o700);
  const env = {
    PATH: process.env.PATH, HOME: home, USERPROFILE: home,
    XDG_CONFIG_HOME: join(root, "config"), XDG_DATA_HOME: join(root, "data"),
    XDG_CACHE_HOME: join(root, "cache"), XDG_STATE_HOME: join(root, "state"),
    OPENCODE_DB: join(root, "opencode.sqlite"), OPENCODE_DISABLE_FREE_MODELS: "1",
    TRUESPEC_BIN: binary, TRUESPEC_NO_OPEN: "1", TRUESPEC_SERVICE_PORT: String(servicePort),
    TRUESPEC_TAILSCALE_BIN: fakeTailscale,
  };
  serviceEnv = env;
  const tailnetStarted = await new Promise(resolve => {
    execFile(binary, ["up", "--tailnet", "--tailnet-port", "18443", "--json"], {
      env, cwd: workspace, encoding: "utf8", timeout: 25_000,
    }, (error, stdout, stderr) => resolve({ ok: !error, text: stdout.trim() || stderr.trim() }));
  });
  assert.ok(tailnetStarted.ok, tailnetStarted.text);
  const initialService = JSON.parse(tailnetStarted.text);
  assert.equal(initialService.tailnet.enabled, true);
  assert.equal(initialService.tailnet.verified, true);
  const initialized = await new Promise(resolve => {
    execFile(binary, ["init", "--integration", "opencode"], {
      env, cwd: workspace, encoding: "utf8", timeout: 25_000,
    }, (error, stdout, stderr) => resolve({ ok: !error, text: stdout.trim() || stderr.trim() }));
  });
  assert.ok(initialized.ok, initialized.text);
  server = spawn(opencode, ["serve", "--service", "--hostname", "127.0.0.1", "--port", "0"],
    { env, cwd: workspace, stdio: ["ignore", "pipe", "pipe"] });
  server.stdout.on("data", chunk => { log = (log + chunk).slice(-16_000); });
  server.stderr.on("data", chunk => { log = (log + chunk).slice(-16_000); });
  server.on("error", error => { providerError = error; });
  await until("isolated OpenCode service registration", () =>
    access(join(env.XDG_STATE_HOME, "opencode/service.json")).then(() => true, () => false));
  // The official client discovers and authenticates to the isolated service.
  const cliRequest = (method, path, body) => new Promise(resolve => {
    const args = ["api", method, path, ...(body === undefined ? [] : ["--data", JSON.stringify(body)])];
    execFile(opencode, args, { env, cwd: workspace, encoding: "utf8", timeout: 25_000, maxBuffer: 8 * 1024 * 1024 },
      (error, stdout, stderr) => resolve({ ok: !error, text: stdout.trim() || stderr.trim() }));
  });
  const openCode = async (path, body) => {
    const response = await cliRequest(body === undefined ? "GET" : "POST", path, body);
    assert.ok(response.ok, `${path}: ${response.text}\n${log}`);
    return JSON.parse(response.text);
  };

  const { data: session } = await openCode("/api/session", { title: "Paired review", location: { directory: workspace } });
  const { data: other } = await openCode("/api/session", { title: "Not paired", location: { directory: workspace } });
  const awayWorkspace = join(root, "away-workspace");
  await mkdir(join(awayWorkspace, ".git"), { recursive: true });
  const chat = `/api/session/${session.id}`;
  const command = { name: "truespec-review", text: "" };
  const invoke = path => cliRequest("POST", `${path}/command`, command);
  const opened = await invoke(chat);
  assert.ok(opened.ok, `${opened.text}\n${log}`);
  const notesFor = async path => {
    const inbox = await openCode(`${path}/inbox`);
    const messages = await openCode(`${path}/message?type=synthetic`);
    return [...new Set(JSON.stringify([inbox, messages]).match(
      /(?:http:\/\/127\.0\.0\.1:\d+|https:\/\/device\.example\.ts\.net:18443)\/w\/[a-z0-9-]+\/\?review=[a-z0-9-]+/g,
    ) || [])];
  };
  const getNotes = () => notesFor(chat);
  const selectedViewer = await until("TrueSpec viewer URL published to the paired chat", async () => (await getNotes())[0]);
  assert.match(selectedViewer, /^https:\/\/device\.example\.ts\.net:18443\//);
  let selected = new URL(selectedViewer);
  let viewer = new URL(`${selected.pathname}${selected.search}`, initialService.endpoint).href;
  const review = selected.searchParams.get("review");
  assert.ok(review);
  const competing = await invoke(`/api/session/${other.id}`);
  assert.ok(!competing.ok && /different chat/.test(competing.text), `Pairing a different chat: ${competing.text}`);
  const reopened = await invoke(chat);
  assert.ok(reopened.ok, reopened.text);
  assert.deepEqual(await getNotes(), [selectedViewer]);

  const route = `/api/reviews/${review}`;
  const board = () => api(viewer, route);
  assert.equal((await board()).review.connector, "OpenCode");
  const [document] = await api(viewer, "/api/documents");
  assert.equal(document.path, reviewedPath);
  const page = () => api(viewer, `/api/documents/${document.id}`);
  const round = async (message, expected, end, verifyMove = false) => {
    const before = await page();
    if (verifyMove) {
      const moved = await cliRequest("POST", `${chat}/move`, { directory: awayWorkspace });
      assert.ok(moved.ok, moved.text);
      await until("the paired chat to move away", async () =>
        (await openCode(chat)).data.location.directory === awayWorkspace);
    }
    const item = await api(viewer, `${route}/comments`, {
      document_id: document.id, revision: before.revision, target: { kind: "line", line: 3 }, body: message,
    }, 201);
    const batch = await api(viewer, `${route}/submit`, {
      key: randomUUID(), drafts: [{ id: item.id, version: 1 }], end,
    });
    if (verifyMove) {
      await until("the moved-session delivery to defer", async () => {
        const state = await board();
        return state.comments.find(comment => comment.id === item.id)?.delivery === "connector_retry";
      });
      assert.equal((await page()).revision, before.revision);
      const movedMessages = JSON.stringify(await openCode(`${chat}/message?type=user`));
      assert.doesNotMatch(movedMessages, new RegExp(batch.batch_id.replaceAll("-", "")));
      const restored = await cliRequest("POST", `${chat}/move`, { directory: workspace });
      assert.ok(restored.ok, restored.text);
      await until("the paired chat to return", async () =>
        (await openCode(chat)).data.location.directory === workspace);
    }
    await until(`OpenCode to apply ${expected}`, async () => (await page()).html.includes(expected));
    assert.notEqual((await page()).revision, before.revision);
    const mirrored = await until("the mirrored agent reply", async () => {
      const state = await board();
      const reply = state.replies?.find(reply => reply.batch_id === batch.batch_id);
      return reply && state.comments.find(comment => comment.id === item.id)?.delivery === "replied"
        ? { state, reply } : undefined;
    });
    assert.match(mirrored.reply.body, new RegExp(expected.replace(".", "\\.")));
    assert.deepEqual(mirrored.reply.comment_ids, [item.id]);
    assert.equal((await board()).review.ended, end);
    const messages = await openCode(`${chat}/message?type=user`);
    const id = `msg_truespec_${batch.batch_id.replaceAll("-", "")}`;
    const delivered = JSON.stringify(messages);
    assert.equal(delivered.split(id).length - 1, 1, "The batch belongs to one chat message");
    assert.match(delivered, /@truespec-review/);
    assert.doesNotMatch(delivered, /Treat each comment|Do not edit Markdown|Read the current files/);
    return batch;
  };
  const firstBatch = await round("Use revision one.", "Revision one.", false, true);

  const markdownWorkspace = join(root, "markdown-workspace");
  await mkdir(join(markdownWorkspace, ".git"), { recursive: true });
  await mkdir(join(markdownWorkspace, ".opencode/plugins"), { recursive: true });
  await writeFile(join(markdownWorkspace, "guide.md"), "# Markdown guide\n");
  await writeFile(join(markdownWorkspace, ".opencode/plugins/rewrite-fixture.ts"), `
    import { Plugin } from "@opencode/plugin"
    import { writeFile } from "node:fs/promises"
    import { join } from "node:path"
    export default Plugin.define({
      id: "rewrite-markdown-fixture",
      async setup(ctx) {
        await ctx.tool.transform(editor => editor.add({
          name: "rewrite_fixture",
          description: "Update the deterministic Markdown fixture",
          input: { type: "object", properties: { revision: { enum: ["Markdown revision."] } },
            required: ["revision"], additionalProperties: false },
          options: { codemode: false },
          async execute({ revision }) {
            await writeFile(join(ctx.location.directory, "guide.md"), "# Markdown guide\\n\\n" + revision + "\\n")
            return { content: "Updated the Markdown fixture." }
          },
        }))
      },
    })
  `);
  await writeFile(join(markdownWorkspace, "opencode.json"), JSON.stringify({
    model: "fixture/mock",
    providers: {
      fixture: {
        package: "@opencode/ai/providers/openai-compatible",
        settings: { baseURL: providerUrl, apiKey: "test-only", timeout: 8_000 },
        compatibility: { streamUsage: false },
        models: { mock: { capabilities: { tools: true } } },
      },
    },
  }));
  const markdownInit = await new Promise(resolve => {
    execFile(binary, [
      "init", "--source", "markdown", "--source-path", ".", "--integration", "opencode",
    ], { env, cwd: markdownWorkspace, encoding: "utf8", timeout: 25_000 },
    (error, stdout, stderr) => resolve({ ok: !error, text: stdout.trim() || stderr.trim() }));
  });
  assert.ok(markdownInit.ok, markdownInit.text);
  const { data: markdownSession } = await openCode("/api/session", {
    title: "Markdown review", location: { directory: markdownWorkspace },
  });
  const markdownChat = `/api/session/${markdownSession.id}`;
  const markdownOpened = await invoke(markdownChat);
  assert.ok(markdownOpened.ok, `${markdownOpened.text}\n${log}`);
  const selectedMarkdownViewer = await until(
    "Markdown TrueSpec viewer URL",
    async () => (await notesFor(markdownChat))[0],
  );
  assert.match(selectedMarkdownViewer, /^https:\/\/device\.example\.ts\.net:18443\//);
  selected = new URL(selectedMarkdownViewer);
  const markdownViewer = new URL(`${selected.pathname}${selected.search}`, initialService.endpoint).href;
  assert.notEqual(new URL(markdownViewer).pathname, new URL(viewer).pathname);
  assert.equal((await board()).review.ended, false);
  const markdownDocuments = await api(markdownViewer, "/api/documents");
  const markdownDocument = markdownDocuments.find(document => document.path === "guide.md");
  assert.equal(markdownDocument?.kind, "markdown");
  const markdownRoute = `/api/reviews/${new URL(markdownViewer).searchParams.get("review")}`;
  const markdownBefore = await api(markdownViewer, `/api/documents/${markdownDocument.id}`);
  const markdownComment = await api(markdownViewer, `${markdownRoute}/comments`, {
    document_id: markdownDocument.id,
    revision: markdownBefore.revision,
    target: { kind: "line", line: 1 },
    body: "Use markdown revision.",
  }, 201);
  const markdownBatch = await api(markdownViewer, `${markdownRoute}/submit`, {
    key: randomUUID(),
    drafts: [{ id: markdownComment.id, version: 1 }],
    end: true,
  });
  await until("OpenCode to apply the second repository feedback", async () =>
    (await api(markdownViewer, `/api/documents/${markdownDocument.id}`)).html.includes("Markdown revision."));
  const markdownBoard = await until("the second repository mirrored reply", async () => {
    const state = await api(markdownViewer, markdownRoute);
    return state.replies?.some(reply => reply.batch_id === markdownBatch.batch_id) ? state : undefined;
  });
  assert.equal(markdownBoard.replies.length, 1);
  assert.equal(markdownBoard.review.ended, true);
  const markdownMessages = JSON.stringify(await openCode(`${markdownChat}/message?type=user`));
  const markdownMessageID = `msg_truespec_${markdownBatch.batch_id.replaceAll("-", "")}`;
  assert.equal(markdownMessages.split(markdownMessageID).length - 1, 1);
  assert.doesNotMatch(markdownMessages, new RegExp(firstBatch.batch_id));

  const lifecycle = args => new Promise(resolve => {
    execFile(binary, args, { env, cwd: workspace, encoding: "utf8", timeout: 25_000 },
      (error, stdout, stderr) => resolve({
        ok: !error,
        text: stdout.trim() || stderr.trim(),
      }));
  });
  const beforeRestart = JSON.parse((await lifecycle(["status", "--json"])).text);
  assert.equal(beforeRestart.tailnet.enabled, true);
  assert.equal((await lifecycle(["down", "--json"])).ok, true);
  const reopenedAfterRestart = await invoke(chat);
  assert.ok(reopenedAfterRestart.ok, reopenedAfterRestart.text);
  const afterRestart = await until("shared service recovery after restart", async () => {
    const result = await lifecycle(["status", "--json"]);
    if (!result.ok) return undefined;
    const status = JSON.parse(result.text);
    return status.state === "running" && status.pid !== beforeRestart.pid ? status : undefined;
  });
  assert.equal(afterRestart.state, "running");
  assert.equal(afterRestart.tailnet.enabled, false);
  assert.notEqual(afterRestart.pid, beforeRestart.pid);
  const previousViewer = new URL(viewer);
  viewer = new URL(`${previousViewer.pathname}${previousViewer.search}`, afterRestart.endpoint).href;

  const secondBatch = await round("Use revision two.", "Revision two.", true);
  assert.equal((await board()).comments.filter(comment => comment.delivery === "replied").length, 2);
  assert.equal((await board()).replies.length, 2);
  const firstMessages = JSON.stringify(await openCode(`${chat}/message?type=user`));
  assert.doesNotMatch(firstMessages, new RegExp(markdownBatch.batch_id));
  assert.match(firstMessages, new RegExp(firstBatch.batch_id.replaceAll("-", "")));
  assert.match(firstMessages, new RegExp(secondBatch.batch_id.replaceAll("-", "")));

  const otherMessages = await openCode(`/api/session/${other.id}/message?type=user`);
  assert.doesNotMatch(JSON.stringify(otherMessages), /@truespec-review/);
  const exit = once(server, "exit");
  server.kill();
  await exit;
  server = undefined;
  assert.equal(JSON.parse((await lifecycle(["status", "--json"])).text).state, "running");
  console.log("Verified two concurrent OpenCode repositories, service recovery, and isolated replies.");
} finally {
  if (server && server.exitCode === null && server.signalCode === null) {
    const exit = once(server, "exit");
    server.kill();
    const timer = setTimeout(() => server.kill("SIGKILL"), 5000);
    try { await exit; }
    finally { clearTimeout(timer); }
  }
  provider.closeAllConnections();
  provider.close();
  if (serviceEnv) {
    await new Promise(resolve => {
      execFile(binary, ["down", "--json"], { env: serviceEnv, encoding: "utf8", timeout: 15_000 },
        () => resolve());
    });
  }
  await rm(root, { recursive: true, force: true });
}
