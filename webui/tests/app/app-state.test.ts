import { createRoot, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { describe, expect, it, vi } from "vitest";
import { createOverlayManager } from "../../src/app/createOverlayManager";
import { createPageNavigation, type PageId } from "../../src/app/createPageNavigation";

describe("app navigation and overlays", () => {
  it("projects page activation into history without adding a Back entry", () => {
    createRoot((dispose) => {
      window.history.replaceState({}, "");
      const replaceState = vi.spyOn(window.history, "replaceState");
      const navigation = createPageNavigation({ browserWindow: window });
      navigation.activatePage("tools");
      expect(navigation.activePage()).toBe("tools");
      expect(navigation.pageProgress()).toBe(1);
      expect(replaceState).toHaveBeenLastCalledWith(expect.objectContaining({ usbSrPage: "tools" }), "");
      replaceState.mockRestore();
      dispose();
    });
  });

  it("gives confirmation Back handling priority over other overlays", () => {
    window.history.replaceState({}, "");
    const host = document.createElement("div");
    let overlays!: ReturnType<typeof createOverlayManager>;
    let setConfirmOpen!: (value: boolean) => void;
    const dismissed = vi.fn(() => setConfirmOpen(false));
    const dispose = render(() => {
      const [confirmOpen, setOpen] = createSignal(false);
      setConfirmOpen = setOpen;
      overlays = createOverlayManager({
        activePage: () => "policy",
        confirmOpen,
        onConfirmDismiss: dismissed,
        browserWindow: window
      });
      return null;
    }, host);
    overlays.openOverlay("log");
    setConfirmOpen(true);
    overlays.openConfirmHistory();
    window.dispatchEvent(new PopStateEvent("popstate"));
    expect(dismissed).toHaveBeenCalledOnce();
    expect(overlays.logOpen()).toBe(true);
    window.dispatchEvent(new PopStateEvent("popstate"));
    expect(overlays.logOpen()).toBe(false);
    dispose();
  });
});

it("removes policy from navigation, remaps indices and prevents activation of hidden pages", () => {
  const host = document.createElement("div");
  let navigation!: ReturnType<typeof createPageNavigation>;
  let setPages!: (pages: readonly PageId[]) => void;
  const dispose = render(() => {
    const [pages, update] = createSignal<readonly PageId[]>(["policy", "tools", "tuning", "settings"]);
    setPages = update;
    navigation = createPageNavigation({ pages });
    return null;
  }, host);
  expect(navigation.activePage()).toBe("policy");
  setPages(["tools", "tuning", "settings"]);
  expect(navigation.activePage()).toBe("tools");
  expect(navigation.activeIndex()).toBe(0);
  expect(navigation.pageProgress()).toBe(0);
  navigation.activatePage("policy");
  expect(navigation.activePage()).toBe("tools");
  navigation.activatePage("settings");
  expect(navigation.activeIndex()).toBe(2);
  expect(window.history.state.usbSrPage).toBe("settings");
  setPages(["policy", "tools", "tuning", "settings"]);
  expect(navigation.activePage()).toBe("settings");
  expect(navigation.activeIndex()).toBe(3);
  dispose();
});
