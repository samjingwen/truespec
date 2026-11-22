import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import { access, mkdir, mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
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
  const response = await fetch(new URL(path, origin), body === undefined ? {} : {
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
    const round = text.includes("Use revision two.") ? "Revision two." : text.includes("Use revision one.") ? "Revision one." : null;
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
  const env = {
    PATH: process.env.PATH, HOME: home, USERPROFILE: home,
    XDG_CONFIG_HOME: join(root, "config"), XDG_DATA_HOME: join(root, "data"),
    XDG_CACHE_HOME: join(root, "cache"), XDG_STATE_HOME: join(root, "state"),
    OPENCODE_DB: join(root, "opencode.sqlite"), OPENCODE_DISABLE_FREE_MODELS: "1",
    TRUESPEC_BIN: binary, TRUESPEC_NO_OPEN: "1",
  };
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
  const chat = `/api/session/${session.id}`;
  const command = { name: "truespec-review", text: "" };
  const invoke = path => cliRequest("POST", `${path}/command`, command);
  const opened = await invoke(chat);
  assert.ok(opened.ok, `${opened.text}\n${log}`);
  const notesFor = async path => {
    const inbox = await openCode(`${path}/inbox`);
    const messages = await openCode(`${path}/message?type=synthetic`);
    return [...new Set(JSON.stringify([inbox, messages]).match(/http:\/\/127\.0\.0\.1:\d+\/\?review=[a-z0-9-]+/g) || [])];
  };
  const getNotes = () => notesFor(chat);
  const viewer = await until("TrueSpec viewer URL published to the paired chat", async () => (await getNotes())[0]);
  const review = new URL(viewer).searchParams.get("review");
  assert.ok(review);
  const competing = await invoke(`/api/session/${other.id}`);
  assert.ok(!competing.ok && /different chat/.test(competing.text), `Pairing a different chat: ${competing.text}`);
  const reopened = await invoke(chat);
  assert.ok(reopened.ok, reopened.text);
  assert.deepEqual(await getNotes(), [viewer]);

  const route = `/api/reviews/${review}`;
  const board = () => api(viewer, route);
  assert.equal((await board()).review.connector, "OpenCode");
  const [document] = await api(viewer, "/api/documents");
  assert.equal(document.path, reviewedPath);
  const page = () => api(viewer, `/api/documents/${document.id}`);
  const round = async (message, expected, end) => {
    const before = await page();
    const item = await api(viewer, `${route}/comments`, {
      document_id: document.id, revision: before.revision, target: { kind: "line", line: 3 }, body: message,
    }, 201);
    const batch = await api(viewer, `${route}/submit`, {
      key: randomUUID(), drafts: [{ id: item.id, version: 1 }], end,
    });
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
  };
  await round("Use revision one.", "Revision one.", false);
  await round("Use revision two.", "Revision two.", true);
  assert.equal((await board()).comments.filter(comment => comment.delivery === "replied").length, 2);
  assert.equal((await board()).replies.length, 2);

  const markdownWorkspace = join(root, "markdown-workspace");
  await mkdir(join(markdownWorkspace, ".git"), { recursive: true });
  await writeFile(join(markdownWorkspace, "guide.md"), "# Markdown guide\n");
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
  const markdownViewer = await until(
    "Markdown TrueSpec viewer URL",
    async () => (await notesFor(markdownChat))[0],
  );
  const markdownDocuments = await api(markdownViewer, "/api/documents");
  assert.equal(markdownDocuments.find(document => document.path === "guide.md")?.kind, "markdown");

  const otherMessages = await openCode(`/api/session/${other.id}/message?type=user`);
  assert.doesNotMatch(JSON.stringify(otherMessages), /@truespec-review/);
  console.log("Verified paired OpenCode review rounds and both initialized source adapters.");
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
  await rm(root, { recursive: true, force: true });
}
