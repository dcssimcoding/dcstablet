const vm = require("node:vm"),
  fs = require("node:fs"),
  path = require("node:path"),
  assert = require("node:assert/strict");
const test = require("node:test");
const script = fs
  .readFileSync(path.join(__dirname, "../web/index.html"), "utf8")
  .match(/<script>([\s\S]*?)<\/script>/)[1];
function harness() {
  const elements = new Map(),
    peers = [];
  const element = (s) => {
    if (!elements.has(s))
      elements.set(s, {
        textContent: "",
        disabled: false,
        classList: { add() {}, remove() {}, toggle() {} },
        play: async () => {},
        setAttribute() {},
      });
    return elements.get(s);
  };
  let status = {
    backend: "rust",
    process_id: 10,
    build_id: "test",
    version: "0.1.0",
    build_date: "2026-10-28",
    mode: "capture",
    fps: 30,
    size: 512,
    last_error: null,
    controls: { session: 1, ready: true, bridge_connected: true },
  };
  let hook = null;
  class Peer {
    constructor() {
      this.connectionState = "new";
      this.iceGatheringState = "complete";
      peers.push(this);
    }
    addTransceiver() {}
    async createOffer() {
      return { type: "offer", sdp: "test" };
    }
    async setLocalDescription(d) {
      this.localDescription = d;
    }
    async setRemoteDescription() {
      this.connectionState = "connected";
      this.onconnectionstatechange?.();
    }
    close() {
      this.closed = true;
      this.connectionState = "closed";
      this.onconnectionstatechange?.();
    }
    async getStats() {
      return new Map();
    }
  }
  const context = {
    document: {
      documentElement: { style: { setProperty() {} } },
      querySelector: element,
      querySelectorAll: () => [],
      visibilityState: "visible",
      addEventListener() {},
    },
    window: { RTCPeerConnection: Peer, addEventListener() {} },
    RTCPeerConnection: Peer,
    navigator: {},
    AbortController,
    AbortSignal,
    DOMException,
    MediaStream: class {},
    console,
    setInterval: () => 1,
    clearInterval() {},
    setTimeout: () => 1,
    clearTimeout() {},
    fetch: async (url, options) => {
      if (hook) {
        const r = hook(url, options);
        if (r) return r;
      }
      return url === "/offer"
        ? { ok: true, json: async () => ({ type: "answer", sdp: "test" }) }
        : { ok: true, json: async () => JSON.parse(JSON.stringify(status)) };
    },
  };
  vm.createContext(context);
  vm.runInContext(script, context);
  return {
    context,
    element,
    peers,
    setStatus: (s) => (status = s),
    status: () => status,
    hook: (f) => (hook = f),
    run: (s) => vm.runInContext(s, context),
  };
}
const tick = () => new Promise(setImmediate);

test("playback rejection releases the viewer and enables retry", async () => {
  const h = harness();
  await tick();
  await h.element("#connect").onclick();
  h.element("#source-video").play = async () => {
    throw Error("Playback blocked");
  };
  await h.peers[0].ontrack({ track: {} });
  assert.equal(h.element("#connect").disabled, false);
  assert.equal(h.peers[0].closed, true);
  assert.match(h.element("#message").textContent, /Playback could not start/);
  await h.element("#connect").onclick();
  assert.equal(h.peers[1].closed, undefined);
});
test("a pre-reconnect status response cannot close or reset the new session", async () => {
  const h = harness();
  await tick();
  await h.element("#connect").onclick();
  const old = h.status();
  let resolve;
  h.hook((url) =>
    url === "/status" ? new Promise((r) => (resolve = r)) : null,
  );
  const pending = h.context.updateStatus();
  assert.equal(
    h.context.updateStatus(),
    pending,
    "status polling must be single-flight",
  );
  h.context.stop();
  h.hook(() => null);
  h.setStatus({
    ...old,
    process_id: 11,
    controls: { ...old.controls, session: 2 },
  });
  await h.element("#connect").onclick();
  resolve({ ok: true, json: async () => old });
  await pending;
  assert.equal(h.peers[1].closed, undefined);
  assert.equal(h.run("controlSession"), 2);
  assert.equal(h.run("controlReady"), true);
});
for (const outcome of ["resolve", "reject"])
  test(`old control ${outcome} cannot alter new input`, async () => {
    const h = harness();
    await tick();
    await h.element("#connect").onclick();
    let settle;
    h.hook((url) =>
      url === "/controls"
        ? new Promise(
            (resolve, reject) =>
              (settle =
                outcome === "reject"
                  ? () => reject(Error("Old timeout"))
                  : () => resolve({ ok: true })),
          )
        : null,
    );
    h.run("queueControls()");
    h.context.stop();
    h.setStatus({
      ...h.status(),
      controls: { session: 2, ready: true, bridge_connected: true },
    });
    let newRequests = 0;
    h.hook((url) => {
      if (url === "/controls") {
        newRequests++;
        return Promise.resolve({ ok: true });
      }
      return null;
    });
    await h.element("#connect").onclick();
    h.run("queueControls()");
    await tick();
    assert.equal(newRequests, 1, "new input must not wait for the old request");
    settle();
    await tick();
    assert.equal(h.run("controlReady"), true);
    assert.equal(h.run("controlSession"), 2);
  });
