import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "../../src/app/App";
import { controllerClient } from "../../src/platform/controller-client";
import { deviceSchema } from "../fixtures/device-schema";

// jsdom has no layout engine. Model Embla's selected snap and reInit while
// testing the real application DOM, card gates, history and navigation indices.
vi.mock("embla-carousel", () => ({ default: (viewport: HTMLElement) => {
  let selected = 0;
  const callbacks = new Map<string, () => void>();
  const api = {
    on: (event: string, callback: () => void) => { callbacks.set(event, callback); return api; },
    destroy: () => {},
    reInit: (options: { startIndex: number }) => { selected = options.startIndex; },
    scrollTo: (index: number) => { selected = index; callbacks.get("select")?.(); },
    scrollProgress: () => selected / Math.max(1, viewport.querySelectorAll(".page-panel").length - 1),
    selectedScrollSnap: () => selected
  };
  return api;
} }));

beforeEach(() => {
  // Node's optional localStorage global shadows jsdom's storage in this runtime.
  Object.defineProperty(window, "localStorage", { configurable: true,
    value: { getItem: () => "en", setItem: () => {} } });
});

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; vi.restoreAllMocks(); document.body.replaceChildren(); });
const result = { code: 0, stdout: "", stderr: "" };

it("hides restricted cards and policy, retains all jitter, and remaps full/limited navigation", async () => {
  vi.spyOn(controllerClient, "status").mockResolvedValue({ result, status: {
    policy: "offload", policy_configured: "1", sample_rate: "192000",
    audioserver_pid: "42", bluetooth_a2dp_state: "connected", state_degraded: "1", state_degraded_reason: "fixture state error"
  } });
  const schema = vi.spyOn(controllerClient, "schema").mockResolvedValue({ result, schema: deviceSchema(true) });
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <App />, host);
  await vi.waitFor(() => expect(host.querySelectorAll(".extra-grid > .card")).toHaveLength(2));
  expect(host.querySelector('[data-page="policy"]')).toBeNull();
  expect(host.querySelectorAll(".page-nav-item")).toHaveLength(3);
  expect(host.querySelectorAll('.jitter-grid input[type="checkbox"]')).toHaveLength(11);
  expect(host.textContent).toContain("fixture state error");
  expect(host.querySelector(".status-strip")?.textContent).toContain("42");

  schema.mockResolvedValue({ result, schema: deviceSchema(false) });
  (host.querySelector(".app-header .icon-button") as HTMLButtonElement).click();
  await vi.waitFor(() => expect(host.querySelectorAll(".page-nav-item")).toHaveLength(4));
  expect(host.querySelectorAll(".extra-grid > .card")).toHaveLength(4);
  expect(host.querySelector('[data-page="policy"]')).not.toBeNull();
  (host.querySelectorAll(".page-nav-item")[3] as HTMLButtonElement).click();
  expect(window.history.state.usbSrPage).toBe("settings");

  await vi.waitFor(() => expect((host.querySelector(".app-header .icon-button") as HTMLButtonElement).disabled).toBe(false));
  schema.mockResolvedValue({ result, schema: deviceSchema(true) });
  (host.querySelector(".app-header .icon-button") as HTMLButtonElement).click();
  await vi.waitFor(() => expect(host.querySelectorAll(".page-nav-item")).toHaveLength(3));
  expect(host.querySelectorAll(".page-nav-item")[2].getAttribute("aria-current")).toBe("page");
  expect(window.history.state.usbSrPage).toBe("settings");
  expect(host.querySelectorAll(".extra-grid > .card")).toHaveLength(2);
});

it("never exposes write controls while schema is missing or malformed", async () => {
  vi.spyOn(controllerClient, "status").mockResolvedValue({ result, status: {} });
  vi.spyOn(controllerClient, "schema").mockResolvedValue({ result });
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <App />, host);
  await Promise.resolve();
  expect(host.querySelector('[data-page="policy"]')).toBeNull();
  expect(host.querySelectorAll(".extra-grid > .card")).toHaveLength(0);
  expect(host.querySelectorAll('.jitter-grid input[type="checkbox"]')).toHaveLength(0);
});
