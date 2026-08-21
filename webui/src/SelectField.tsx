import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { Portal } from "solid-js/web";
import { CheckIcon } from "./Icons";

export type SelectOption = readonly [value: string, label: string];

export default function SelectField(props: {
  id?: string;
  title: string;
  value: string;
  options: ReadonlyArray<SelectOption>;
  disabled?: boolean;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const selectedLabel = createMemo(() => props.options.find(([value]) => value === props.value)?.[1] ?? props.value);
  const historyKey = `select-${Math.random().toString(36).slice(2)}`;
  let optionList: HTMLDivElement | undefined;
  let ownsHistoryEntry = false;

  function openDialog() {
    if (open()) return;
    window.history.pushState({ ...window.history.state, usbSrOverlay: historyKey }, "");
    ownsHistoryEntry = true;
    setOpen(true);
  }

  function closeDialog(fromHistory = false) {
    setOpen(false);
    if (!fromHistory && ownsHistoryEntry && window.history.state?.usbSrOverlay === historyKey) {
      window.history.back();
    }
    ownsHistoryEntry = false;
  }

  onMount(() => {
    const handlePopState = () => {
      if (!open()) return;
      closeDialog(true);
    };
    window.addEventListener("popstate", handlePopState);
    onCleanup(() => window.removeEventListener("popstate", handlePopState));
  });

  createEffect(() => {
    if (!open()) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeDialog();
    };
    document.addEventListener("keydown", closeOnEscape);
    const frame = window.requestAnimationFrame(() => {
      optionList?.querySelector<HTMLElement>(".select-option.selected")?.scrollIntoView({ block: "center" });
    });
    onCleanup(() => {
      document.removeEventListener("keydown", closeOnEscape);
      window.cancelAnimationFrame(frame);
    });
  });

  return (
    <>
      <button
        id={props.id}
        type="button"
        class="select-trigger"
        aria-haspopup="listbox"
        aria-expanded={open()}
        disabled={props.disabled}
        onClick={openDialog}
      >
        <span>{selectedLabel()}</span><span class="select-chevron" aria-hidden="true" />
      </button>
      <Show when={open()}>
        <Portal>
          <div class="select-dialog-layer" data-no-page-drag role="presentation">
            <button class="select-dialog-backdrop" aria-label={`关闭${props.title}选择`} onClick={() => closeDialog()} />
            <section class="select-dialog" role="dialog" aria-modal="true" aria-label={props.title}>
              <header><h2>{props.title}</h2><button type="button" class="dialog-close" aria-label={`关闭${props.title}选择`} onClick={() => closeDialog()}>×</button></header>
              <div class="select-option-list" role="listbox" aria-label={props.title} ref={optionList}>
                <For each={props.options}>{([value, label]) => (
                  <button
                    type="button"
                    role="option"
                    class="select-option"
                    classList={{ selected: value === props.value }}
                    aria-selected={value === props.value}
                    onClick={() => { props.onChange(value); closeDialog(); }}
                  >
                    <span>{label}</span><span class="select-check"><Show when={value === props.value}><CheckIcon /></Show></span>
                  </button>
                )}</For>
              </div>
            </section>
          </div>
        </Portal>
      </Show>
    </>
  );
}
