import { Plugin } from "@opencode/plugin";
import { execFile, spawn, type ChildProcess } from "node:child_process";
import { realpath } from "node:fs/promises";
import { setTimeout as delay } from "node:timers/promises";

const PROTOCOL = 1;
const MAX_OUTPUT = 32 * 1024 * 1024;
const INSTALLED_BINARY = "__TRUESPEC_INSTALLED_BINARY__";

type Binding = {
  connection_id: string;
  review_id: string;
  workspace: string;
  sessionID: string;
  chatDirectory: string;
};

type Viewer = { url: string; child: ChildProcess };

type PendingReply = {
  connection_id: string;
  review_id: string;
  batch_id: string;
  workspace: string;
  sessionID: string;
  userMessageID: string;
};

type Delivery = {
  event: "batch";
  review_id: string;
  batch_id: string;
  lease: string;
  attempt: number;
  items: unknown[];
} | { event: "idle" | "ended" };

function record(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function bindingRecord(value: unknown): value is Binding {
  return record(value) && ["connection_id", "review_id", "workspace", "sessionID", "chatDirectory"]
    .every(key => typeof value[key] === "string" && value[key] !== "");
}

function pendingRecord(value: unknown): value is PendingReply {
  return record(value) && ["connection_id", "review_id", "batch_id", "workspace", "sessionID", "userMessageID"]
    .every(key => typeof value[key] === "string" && value[key] !== "");
}

function protocolRecord(text: string): Record<string, unknown> {
  const value: unknown = JSON.parse(text);
  if (!record(value) || value.protocol_version !== PROTOCOL) {
    throw new Error("The installed TrueSpec binary doesn't support connector protocol 1.");
  }
  return value;
}

function deliveryRecord(value: Record<string, unknown>): Delivery {
  if (value.event === "idle" || value.event === "ended") return { event: value.event };
  if (value.event === "batch" && typeof value.review_id === "string" && typeof value.batch_id === "string"
      && typeof value.lease === "string" && typeof value.attempt === "number" && Array.isArray(value.items)) {
    return {
      event: "batch", review_id: value.review_id, batch_id: value.batch_id, lease: value.lease,
      attempt: value.attempt, items: value.items,
    };
  }
  throw new Error("TrueSpec returned an invalid inbox record.");
}

function promptText(batch: Extract<Delivery, { event: "batch" }>, workspace: string): string {
  return [
    "Use @truespec-review to handle this delivered TrueSpec batch.",
    "",
    JSON.stringify({
      reviewed_workspace: workspace,
      review_id: batch.review_id,
      batch_id: batch.batch_id,
      comments: batch.items,
    }, null, 2),
  ].join("\n");
}

function openBrowser(url: string) {
  if (process.env.TRUESPEC_NO_OPEN === "1") return;
  const command = process.platform === "darwin" ? "open" : process.platform === "win32" ? "explorer.exe" : "xdg-open";
  const child = spawn(command, [url], { stdio: "ignore", windowsHide: true });
  child.on("error", () => { /* The chat note also contains the URL. */ });
  child.unref();
}

export default Plugin.define({
  id: "truespec",
  async setup(ctx) {
    const lifetime = new AbortController();
    const { signal } = lifetime;
    const options = record(ctx.options) ? ctx.options : {};
    const binary = typeof options.binary === "string" && options.binary.trim()
      ? options.binary
      : process.env.TRUESPEC_BIN || (INSTALLED_BINARY.startsWith("__") ? "truespec" : INSTALLED_BINARY);
    const viewerByWorkspace = new Map<string, Promise<Viewer>>();
    const bridges = new Map<string, Promise<void>>();
    const mirrors = new Map<string, Promise<void>>();
    const storageKey = `truespec.connections.v1:${ctx.location.directory}`;
    const pendingKey = `truespec.pending-replies.v1:${ctx.location.directory}`;
    let storageWrite: Promise<void> = Promise.resolve();

    function changeBindings(change: (bindings: Binding[]) => Binding[]): Promise<void> {
      const write = storageWrite.then(async () => {
        const known: unknown = await ctx.storage.get(storageKey);
        await ctx.storage.set(storageKey, change(Array.isArray(known) ? known.filter(bindingRecord) : []));
      });
      storageWrite = write.catch(() => {});
      return write;
    }

    function changePending(change: (pending: PendingReply[]) => PendingReply[]): Promise<void> {
      const write = storageWrite.then(async () => {
        const known: unknown = await ctx.storage.get(pendingKey);
        await ctx.storage.set(pendingKey, change(Array.isArray(known) ? known.filter(pendingRecord) : []));
      });
      storageWrite = write.catch(() => {});
      return write;
    }

    function run(args: string[], cwd: string): Promise<Record<string, unknown>> {
      const connector = ["connector", "--protocol", String(PROTOCOL), ...args];
      return new Promise((resolve, reject) => {
        execFile(binary, connector, { cwd, signal, timeout: 40_000, maxBuffer: MAX_OUTPUT, encoding: "utf8", windowsHide: true },
          (error, stdout, stderr) => {
            if (error) {
              reject(new Error(error.code === "ENOENT"
                ? "TrueSpec isn't on the OpenCode server's PATH. Install it or set the plugin's binary option."
                : stderr.trim().slice(0, 500) || "The TrueSpec connector command failed.", { cause: error }));
              return;
            }
            try { resolve(protocolRecord(stdout)); }
            catch (error) { reject(error); }
          });
      });
    }

    function runWithInput(args: string[], cwd: string, input: string): Promise<Record<string, unknown>> {
      const connector = ["connector", "--protocol", String(PROTOCOL), ...args];
      return new Promise((resolve, reject) => {
        const child = spawn(binary, connector, {
          cwd, signal, stdio: ["pipe", "pipe", "pipe"], windowsHide: true,
        });
        let stdout = "";
        let stderr = "";
        child.stdout.setEncoding("utf8");
        child.stderr.setEncoding("utf8");
        child.stdout.on("data", chunk => {
          stdout += chunk;
          if (stdout.length > MAX_OUTPUT) child.kill();
        });
        child.stderr.on("data", chunk => { stderr = (stderr + chunk).slice(-500); });
        child.once("error", reject);
        child.once("exit", code => {
          if (code !== 0) {
            reject(new Error(stderr.trim() || "The TrueSpec connector command failed."));
            return;
          }
          try { resolve(protocolRecord(stdout)); }
          catch (error) { reject(error); }
        });
        child.stdin.end(input);
      });
    }

    function assistantReply(pending: PendingReply, messages: unknown[]): { id: string; body: string } | undefined {
      const start = messages.findIndex(message => record(message) && message.id === pending.userMessageID);
      if (start < 0) return;
      const turn = messages.slice(start + 1);
      const end = turn.findIndex(message => record(message) && message.type === "user");
      const candidates = (end < 0 ? turn : turn.slice(0, end)).filter(message =>
        record(message) && message.type === "assistant" && record(message.time)
          && typeof message.time.completed === "number" && !message.error);
      const message = candidates.at(-1);
      if (!record(message) || typeof message.id !== "string" || !Array.isArray(message.content)) return;
      const text = message.content
        .filter(part => record(part) && part.type === "text" && typeof part.text === "string")
        .map(part => (part as { text: string }).text.trim())
        .filter(Boolean)
        .join("\n\n");
      return {
        id: message.id,
        body: text || "Agent completed this batch without a textual response.",
      };
    }

    async function mirrorSession(sessionID: string) {
      if (mirrors.has(sessionID)) return mirrors.get(sessionID);
      const work = (async () => {
        const stored: unknown = await ctx.storage.get(pendingKey);
        const pending = Array.isArray(stored)
          ? stored.filter(pendingRecord).filter(item => item.sessionID === sessionID)
          : [];
        if (!pending.length) return;
        const messages = await ctx.session.context({ sessionID });
        if (!Array.isArray(messages)) return;
        for (const item of pending) {
          const reply = assistantReply(item, messages);
          if (!reply) continue;
          await runWithInput([
            "respond", item.batch_id,
            "--connection", item.connection_id,
            "--reply-id", reply.id,
            "--body-file", "-",
          ], item.workspace, reply.body);
          await changePending(known => known.filter(entry =>
            !(entry.connection_id === item.connection_id && entry.batch_id === item.batch_id)));
        }
      })().catch(error => {
        if (!signal.aborted) {
          console.warn("TrueSpec could not mirror an agent reply:", error instanceof Error ? error.message : "Unknown error.");
        }
      }).finally(() => mirrors.delete(sessionID));
      mirrors.set(sessionID, work);
      return work;
    }

    function startViewer(workspace: string): Promise<Viewer> {
      return new Promise((resolve, reject) => {
        const child = spawn(binary, ["connector", "--protocol", String(PROTOCOL), "--root", workspace, "serve", "--json"], {
          cwd: workspace, signal, stdio: ["ignore", "pipe", "pipe"], windowsHide: true,
        });
        let pending = true;
        let stdout = "";
        let stderr = "";
        const timer = setTimeout(() => fail(new Error("TrueSpec did not report a viewer URL in time.")), 10_000);
        function fail(error: Error) {
          if (!pending) return;
          pending = false;
          clearTimeout(timer);
          if (child.exitCode === null && child.signalCode === null) child.kill();
          reject(error);
        }
        child.on("error", fail);
        child.once("exit", () => fail(new Error(stderr.trim() || "The TrueSpec viewer exited before it became ready.")));
        child.stderr.setEncoding("utf8");
        child.stderr.on("data", chunk => { stderr = (stderr + chunk).slice(-500); });
        child.stdout.setEncoding("utf8");
        child.stdout.on("data", chunk => {
          if (!pending) return;
          stdout += chunk;
          const newline = stdout.indexOf("\n");
          if (newline < 0 && stdout.length < 8192) return;
          try {
            const ready = protocolRecord(stdout.slice(0, newline < 0 ? stdout.length : newline));
            if (typeof ready.url !== "string" || !/^http:\/\/127\.0\.0\.1:[0-9]+\/$/.test(ready.url)
                || ready.workspace !== workspace) {
              throw new Error("TrueSpec reported an unexpected viewer URL or workspace.");
            }
            pending = false;
            clearTimeout(timer);
            resolve({ url: ready.url, child });
          } catch (error) {
            fail(error instanceof Error ? error : new Error("Invalid viewer startup record."));
          }
        });
      });
    }

    function viewerFor(workspace: string) {
      const existing = viewerByWorkspace.get(workspace);
      if (existing) return existing;
      const started = startViewer(workspace);
      viewerByWorkspace.set(workspace, started);
      const forget = () => {
        if (viewerByWorkspace.get(workspace) === started) viewerByWorkspace.delete(workspace);
      };
      void started.then(({ child }) => child.once("exit", forget), forget);
      return started;
    }

    async function deliver(binding: Binding, batch: Extract<Delivery, { event: "batch" }>) {
      const ids = ["--connection", binding.connection_id, "--lease", batch.lease];
      try {
        const destination = await ctx.session.get({ sessionID: binding.sessionID });
        if (await realpath(destination.location.directory) !== binding.chatDirectory) {
          throw new Error("The paired OpenCode chat moved out of the reviewed location.");
        }
        if (batch.review_id !== binding.review_id) throw new Error("The batch belongs to a different review.");
        // OpenCode deduplicates retried admission of the same message ID.
        const id = `msg_truespec_${batch.batch_id.replaceAll("-", "")}`;
        const receipt = await ctx.session.prompt({
          sessionID: binding.sessionID,
          id,
          text: promptText(batch, binding.workspace),
          delivery: "queue",
          metadata: { truespec: { review_id: binding.review_id, batch_id: batch.batch_id } },
        });
        if (receipt.id !== id || receipt.sessionID !== binding.sessionID) throw new Error("OpenCode returned a mismatched delivery receipt.");
        await changePending(known => [
          ...known.filter(item => !(item.connection_id === binding.connection_id && item.batch_id === batch.batch_id)),
          {
            connection_id: binding.connection_id,
            review_id: binding.review_id,
            batch_id: batch.batch_id,
            workspace: binding.workspace,
            sessionID: binding.sessionID,
            userMessageID: receipt.id,
          },
        ]);
        await run(["delivered", batch.batch_id, ...ids, "--receipt", receipt.id], binding.workspace);
        void ctx.session.wait({ sessionID: binding.sessionID })
          .then(() => mirrorSession(binding.sessionID))
          .catch(error => {
            if (!signal.aborted) console.warn("TrueSpec reply wait failed:", error instanceof Error ? error.message : "Unknown error.");
          });
      } catch (error) {
        if (signal.aborted) return;
        const seconds = Math.min(60, 5 * 2 ** Math.max(0, batch.attempt - 1));
        await run(["defer", batch.batch_id, ...ids, "--retry-after", String(seconds)], binding.workspace).catch(() => {});
        console.warn(`TrueSpec batch ${batch.batch_id} is queued for retry:`, error instanceof Error ? error.message : "Delivery failed.");
      }
    }

    function bridge(binding: Binding) {
      if (bridges.has(binding.connection_id)) return;
      const work = (async () => {
        while (!signal.aborted) {
          try {
            const batch = deliveryRecord(await run(["poll", "--connection", binding.connection_id, "--wait", "20"], binding.workspace));
            if (signal.aborted) break;
            if (batch.event === "ended") {
              await changeBindings(known => known.filter(item => item.connection_id !== binding.connection_id));
              break;
            }
            if (batch.event === "batch") await deliver(binding, batch);
          } catch (error) {
            if (signal.aborted) break;
            console.warn("TrueSpec connector could not read its inbox:", error instanceof Error ? error.message : "Unknown error.");
            await delay(5000, undefined, { signal }).catch(() => {});
          }
        }
      })();
      bridges.set(binding.connection_id, work);
      void work.finally(() => bridges.delete(binding.connection_id));
    }

    await ctx.command.transform(editor => {
      editor.add({
        name: "truespec-review",
        description: "Review this initialized repository in TrueSpec",
        async execute({ sessionID, prompt }) {
          if (prompt.text.trim()) throw new Error("Use /truespec-review without arguments.");
          const chat = await ctx.session.get({ sessionID });
          const chatDirectory = await realpath(chat.location.directory);
          const output = await run(["attach", "--kind", "opencode", "--destination", sessionID, "--label", "OpenCode"], chatDirectory);
          const binding = { ...output, sessionID, chatDirectory };
          if (!bindingRecord(binding)) throw new Error("TrueSpec reported an invalid review connection.");
          const viewer = await viewerFor(binding.workspace);
          await changeBindings(known => [...known.filter(item => item.connection_id !== binding.connection_id), binding]);
          bridge(binding);
          const url = new URL(viewer.url);
          url.searchParams.set("review", binding.review_id);
          openBrowser(url.href);
          await ctx.session.synthetic({
            sessionID,
            text: `TrueSpec review: ${url.href}`,
            description: "TrueSpec review opened",
            resume: false,
          });
        },
      });
    });

    const previous: unknown = await ctx.storage.get(storageKey);
    if (Array.isArray(previous)) previous.filter(bindingRecord).forEach(bridge);
    const pending: unknown = await ctx.storage.get(pendingKey);
    if (Array.isArray(pending)) {
      for (const sessionID of new Set(pending.filter(pendingRecord).map(item => item.sessionID))) {
        void mirrorSession(sessionID);
      }
    }
    void (async () => {
      for await (const event of ctx.event.subscribe({ types: ["session.idle"], signal })) {
        void mirrorSession(event.data.sessionID);
      }
    })();
    return () => lifetime.abort();
  },
});
