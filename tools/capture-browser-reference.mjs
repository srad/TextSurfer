// Captures a browser rendering reference for the offline corpus.
//
// Human-run only: Rust tests never touch the network (AGENTS.md). One run produces both halves of
// a corpus entry — the bytes the browser actually fetched, and the word/box geometry it laid out —
// so the offline comparison renders the identical resource state.
//
// Dependency-free by construction: Node's global fetch reads the DevTools endpoint and its global
// WebSocket speaks CDP, so this repo needs no package.json, node_modules, or npm install. It drives
// the Chromium that Playwright already left on disk.
//
//   node tools/capture-browser-reference.mjs --url https://example.com --slug example --probe
//
// --probe runs the Phase 0 mechanic checks and prints what it found instead of writing a capture.

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CHROME = join(
  process.env.LOCALAPPDATA ?? "",
  "ms-playwright",
  "chromium-1234",
  "chrome-win64",
  "chrome.exe",
);

const WIDTHS = [
  [40, 30],
  [100, 38],
  [160, 45],
];
const COLUMN_PX = 8;
const ROW_PX = 16;

function parseArgs(argv) {
  const args = { probe: false };
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (key === "--probe") args.probe = true;
    else if (key === "--url") args.url = argv[++index];
    else if (key === "--slug") args.slug = argv[++index];
    else throw new Error(`unknown argument ${key}`);
  }
  if (!args.url) throw new Error("--url is required");
  return args;
}

class Cdp {
  #socket;
  #next = 1;
  #pending = new Map();
  #listeners = new Map();

  static async connect(endpoint) {
    const cdp = new Cdp();
    cdp.#socket = new WebSocket(endpoint);
    await new Promise((resolve, reject) => {
      cdp.#socket.addEventListener("open", resolve, { once: true });
      cdp.#socket.addEventListener("error", reject, { once: true });
    });
    cdp.#socket.addEventListener("message", (event) => cdp.#dispatch(JSON.parse(event.data)));
    return cdp;
  }

  #dispatch(message) {
    if (message.id !== undefined) {
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      this.#pending.delete(message.id);
      if (message.error) pending.reject(new Error(`${message.error.message} (${message.error.code})`));
      else pending.resolve(message.result);
      return;
    }
    for (const listener of this.#listeners.get(message.method) ?? []) listener(message.params);
  }

  on(method, listener) {
    if (!this.#listeners.has(method)) this.#listeners.set(method, []);
    this.#listeners.get(method).push(listener);
  }

  once(method) {
    return new Promise((resolve) => {
      const listener = (params) => {
        const listeners = this.#listeners.get(method);
        listeners.splice(listeners.indexOf(listener), 1);
        resolve(params);
      };
      this.on(method, listener);
    });
  }

  send(method, params = {}, sessionId) {
    const id = this.#next++;
    const message = { id, method, params };
    if (sessionId) message.sessionId = sessionId;
    this.#socket.send(JSON.stringify(message));
    return new Promise((resolve, reject) => this.#pending.set(id, { resolve, reject }));
  }

  close() {
    this.#socket.close();
  }
}

async function launch() {
  const profile = await mkdtemp(join(tmpdir(), "textsurfer-capture-"));
  const port = 9800 + Math.floor(Math.random() * 200);
  const child = spawn(
    CHROME,
    [
      "--headless=new",
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${profile}`,
      "--hide-scrollbars",
      "--force-device-scale-factor=1",
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-extensions",
      "--disable-background-networking",
      "about:blank",
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  const endpoint = await waitForEndpoint(port);
  return { child, profile, endpoint };
}

async function waitForEndpoint(port) {
  const deadline = Date.now() + 30_000;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`);
      const body = await response.json();
      if (body.webSocketDebuggerUrl) return body.webSocketDebuggerUrl;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`DevTools endpoint never appeared on port ${port}: ${lastError}`);
}

async function openPage(cdp, cols, rows) {
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  await cdp.send("Page.enable", {}, sessionId);
  await cdp.send("Runtime.enable", {}, sessionId);
  await cdp.send("Network.enable", {}, sessionId);
  await cdp.send("Emulation.setScriptExecutionDisabled", { value: true }, sessionId);
  await cdp.send(
    "Emulation.setDeviceMetricsOverride",
    { width: cols * COLUMN_PX, height: rows * ROW_PX, deviceScaleFactor: 1, mobile: false },
    sessionId,
  );
  await cdp.send(
    "Emulation.setEmulatedMedia",
    { features: [{ name: "prefers-color-scheme", value: "light" }] },
    sessionId,
  );
  return sessionId;
}

function collectResources(cdp, sessionId) {
  const responses = new Map();
  const finished = [];
  cdp.on("Network.responseReceived", (params) => {
    if (params.sessionId && params.sessionId !== sessionId) return;
    responses.set(params.requestId, {
      url: params.response.url,
      status: params.response.status,
      mimeType: params.response.mimeType,
      type: params.type,
    });
  });
  cdp.on("Network.loadingFinished", (params) => finished.push(params.requestId));
  return { responses, finished };
}

async function evaluate(cdp, sessionId, expression) {
  const result = await cdp.send(
    "Runtime.evaluate",
    { expression, returnByValue: true, awaitPromise: true },
    sessionId,
  );
  if (result.exceptionDetails) {
    throw new Error(`page evaluation failed: ${result.exceptionDetails.text}`);
  }
  return result.result.value;
}

const PROBE_EXPRESSION = `(() => ({
  title: document.title,
  scrollY: window.scrollY,
  innerWidth: window.innerWidth,
  innerHeight: window.innerHeight,
  documentWidth: document.documentElement.scrollWidth,
  documentHeight: document.documentElement.scrollHeight,
  scriptRan: window.__textsurferScriptRan === true,
  textLength: document.body ? document.body.innerText.trim().length : 0,
}))()`;

async function probe(url) {
  const { child, profile, endpoint } = await launch();
  const cdp = await Cdp.connect(endpoint);
  try {
    const [cols, rows] = [100, 38];
    const sessionId = await openPage(cdp, cols, rows);
    const resources = collectResources(cdp, sessionId);
    const loaded = cdp.once("Page.loadEventFired");
    await cdp.send("Page.navigate", { url }, sessionId);
    await loaded;
    await new Promise((resolve) => setTimeout(resolve, 500));

    console.log("--- mechanic 1: Runtime.evaluate with script execution disabled ---");
    const facts = await evaluate(cdp, sessionId, PROBE_EXPRESSION);
    console.log(JSON.stringify(facts, null, 2));
    console.log(`expected viewport ${cols * COLUMN_PX}x${rows * ROW_PX}`);
    console.log(
      facts.innerWidth === cols * COLUMN_PX
        ? "viewport width matches (scrollbar is hidden)"
        : `VIEWPORT MISMATCH: got ${facts.innerWidth}`,
    );

    console.log("\n--- mechanic 1b: page script really is disabled ---");
    const scripted = await openPage(cdp, cols, rows);
    const scriptedLoaded = cdp.once("Page.loadEventFired");
    await cdp.send(
      "Page.navigate",
      {
        url:
          "data:text/html," +
          encodeURIComponent(
            "<title>NO SCRIPT</title><p id=t>original</p>" +
              "<script>document.title='SCRIPT RAN';document.getElementById('t').textContent='mutated'</script>",
          ),
      },
      scripted,
    );
    await scriptedLoaded;
    const scriptFacts = await evaluate(
      cdp,
      scripted,
      `(() => ({ title: document.title, text: document.getElementById('t').textContent }))()`,
    );
    console.log(JSON.stringify(scriptFacts));
    console.log(
      scriptFacts.title === "NO SCRIPT" && scriptFacts.text === "original"
        ? "page script did not run, and Runtime.evaluate still works"
        : "SCRIPT LEAKED: setScriptExecutionDisabled did not hold",
    );

    console.log("\n--- mechanic 2: response bodies ---");
    let bodies = 0;
    let failures = 0;
    for (const requestId of resources.finished) {
      const meta = resources.responses.get(requestId);
      if (!meta) continue;
      try {
        const body = await cdp.send("Network.getResponseBody", { requestId }, sessionId);
        const bytes = body.base64Encoded
          ? Buffer.from(body.body, "base64")
          : Buffer.from(body.body, "utf8");
        bodies += 1;
        console.log(
          `  ${meta.status} ${meta.type} ${bytes.length}B sha=${createHash("sha256")
            .update(bytes)
            .digest("hex")
            .slice(0, 12)} ${meta.url.slice(0, 90)}`,
        );
      } catch (error) {
        failures += 1;
        console.log(`  BODY UNAVAILABLE ${meta.url.slice(0, 90)} :: ${error.message}`);
      }
    }
    console.log(`bodies retrieved: ${bodies}, unavailable: ${failures}`);
  } finally {
    cdp.close();
    child.kill();
    await rm(profile, { recursive: true, force: true }).catch(() => {});
  }
}

// Runs in the page. Reduces the render to word tokens with document geometry, because a word is
// the only unit that survives two engines breaking lines with different font metrics.
const EXTRACT = `(() => {
  if (window.scrollY !== 0 || window.scrollX !== 0) throw new Error("page scrolled; geometry would not be document-relative");
  const viewportWidth = window.innerWidth;
  const transparent = (color) => /rgba\\(\\s*\\d+\\s*,\\s*\\d+\\s*,\\s*\\d+\\s*,\\s*0\\s*\\)/.test(color);

  const clipOf = (node) => {
    let hidden = false;
    let clip = { left: -Infinity, top: -Infinity, right: Infinity, bottom: Infinity };
    let direction = "ltr";
    let element = node.parentElement;
    let first = true;
    while (element) {
      const style = getComputedStyle(element);
      if (first) {
        direction = style.direction || "ltr";
        if (transparent(style.color)) hidden = true;
        first = false;
      }
      if (style.display === "none" || style.visibility === "hidden" || Number(style.opacity) === 0) hidden = true;
      const contained = (style.contain || "").split(/\\s+/).some((k) => k === "paint" || k === "content" || k === "strict");
      if (style.overflowX !== "visible" || style.overflowY !== "visible" || contained) {
        const box = element.getBoundingClientRect();
        clip = {
          left: Math.max(clip.left, box.left),
          top: Math.max(clip.top, box.top),
          right: Math.min(clip.right, box.right),
          bottom: Math.min(clip.bottom, box.bottom),
        };
      }
      element = element.parentElement;
    }
    return { hidden, clip, direction };
  };

  const words = [];
  const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const data = node.nodeValue;
    if (!data || !/\\S/.test(data)) continue;
    const { hidden, clip, direction } = clipOf(node);
    for (const match of data.matchAll(/\\S+/g)) {
      const range = document.createRange();
      range.setStart(node, match.index);
      range.setEnd(node, match.index + match[0].length);
      const box = range.getBoundingClientRect();
      range.detach();
      if (box.width === 0 && box.height === 0) continue;
      const clipped =
        box.right <= clip.left || box.left >= clip.right || box.bottom <= clip.top || box.top >= clip.bottom;
      const offscreen = box.right <= 0 || box.left >= viewportWidth;
      // Positional, and rounded to whole CSS pixels: the comparison resolves to 8x16 cells, so
      // sub-pixel precision is a megabyte of noise per page.
      words.push([
        match[0],
        Math.round(box.left),
        Math.round(box.top),
        Math.round(box.width),
        Math.round(box.height),
        !hidden && !clipped && !offscreen ? 1 : 0,
        direction === "rtl" ? 1 : 0,
      ]);
    }
  }

  // Element boxes for the R1 geometry diagnostic are deliberately not captured: nothing reads them
  // yet, and they cost more bytes than every word on the page put together.
  return { words, documentUrl: location.href, documentHeight: document.documentElement.scrollHeight };
})()`;

function extensionFor(mimeType, url) {
  if (mimeType === "text/html") return "html";
  if (mimeType === "text/css") return "css";
  if (mimeType.startsWith("image/")) return mimeType.slice("image/".length).split("+")[0];
  const guess = new URL(url).pathname.split(".").pop();
  return guess && guess.length <= 5 ? guess : "bin";
}

async function capture(url, slug) {
  const { child, profile, endpoint } = await launch();
  const cdp = await Cdp.connect(endpoint);
  const outputDir = join(process.cwd(), "testdata", "browser-corpus", slug);
  const bundle = new Map();
  const references = [];
  try {
    for (const [cols, rows] of WIDTHS) {
      const sessionId = await openPage(cdp, cols, rows);
      const resources = collectResources(cdp, sessionId);
      const loaded = cdp.once("Page.loadEventFired");
      await cdp.send("Page.navigate", { url }, sessionId);
      await loaded;
      await new Promise((resolve) => setTimeout(resolve, 750));

      for (const requestId of resources.finished) {
        const meta = resources.responses.get(requestId);
        if (!meta || bundle.has(meta.url)) continue;
        try {
          const body = await cdp.send("Network.getResponseBody", { requestId }, sessionId);
          const bytes = body.base64Encoded
            ? Buffer.from(body.body, "base64")
            : Buffer.from(body.body, "utf8");
          bundle.set(meta.url, { ...meta, bytes });
        } catch {
          // A body Chromium no longer holds is recorded as absent, not as a silent success.
          bundle.set(meta.url, { ...meta, bytes: null });
        }
      }

      const extracted = await evaluate(cdp, sessionId, EXTRACT);
      references.push({ cols, rows, ...extracted });
      await cdp.send("Page.close", {}, sessionId);
    }
  } finally {
    cdp.close();
    child.kill();
    await rm(profile, { recursive: true, force: true }).catch(() => {});
  }
  return { outputDir, bundle, references };
}

const args = parseArgs(process.argv.slice(2));
if (args.probe) {
  await probe(args.url);
} else {
  if (!args.slug) throw new Error("--slug is required for a capture");
  const { outputDir, bundle, references } = await capture(args.url, args.slug);
  const { writeFile, mkdir } = await import("node:fs/promises");
  await mkdir(outputDir, { recursive: true });
  // The page's own `location.href`, not the URL typed on the command line: a redirect or an added
  // trailing slash otherwise leaves the bundle unable to name its own document.
  const documentUrl = references[0].documentUrl;
  const resources = [];
  let index = 0;
  for (const [resourceUrl, meta] of bundle) {
    if (!meta.bytes) {
      resources.push({ url: resourceUrl, status: meta.status, mimeType: meta.mimeType, file: null });
      continue;
    }
    const file =
      meta.type === "Document" && resourceUrl === documentUrl
        ? `document.${extensionFor(meta.mimeType, resourceUrl)}`
        : `r${String(index++).padStart(3, "0")}.${extensionFor(meta.mimeType, resourceUrl)}`;
    await writeFile(join(outputDir, file), meta.bytes);
    resources.push({
      url: resourceUrl,
      status: meta.status,
      mimeType: meta.mimeType,
      file,
      sha256: createHash("sha256").update(meta.bytes).digest("hex"),
    });
  }
  await writeFile(
    join(outputDir, "bundle.json"),
    `${JSON.stringify({ schema: 1, slug: args.slug, url: documentUrl, requested: args.url, capturedWith: "chromium", resources }, null, 2)}\n`,
  );
  for (const reference of references) {
    await writeFile(
      join(outputDir, `reference-${reference.cols}x${reference.rows}.json`),
      `${JSON.stringify(reference)}\n`,
    );
    const visible = reference.words.filter((word) => word[5] === 1).length;
    console.log(
      `${args.slug} ${reference.cols}x${reference.rows}: ${visible}/${reference.words.length} visible words, document ${reference.documentHeight}px`,
    );
  }
  console.log(`wrote ${resources.length} resources to ${outputDir}`);
}
