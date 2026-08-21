import {
  For,
  Show,
  createMemo,
  createSignal,
  onCleanup,
  onMount
} from "solid-js";
import { render } from "solid-js/web";
import EmblaCarousel, { type EmblaCarouselType } from "embla-carousel";
import "./styles.css";
import SelectField from "./SelectField";
import {
  BrandIcon,
  LogIcon,
  PolicyIcon,
  RefreshIcon,
  ToolsIcon,
  TuneIcon,
  WarningIcon
} from "./Icons";

type ExecResult = { code: number; stdout: string; stderr: string };
type ToastItem = { id: string; message: string; tone?: "success" | "error" };

declare global {
  interface Window {
    __usbSrCallbacks?: Record<string, (code: number, stdout: string, stderr: string) => void>;
    ksu?: {
      exec?: (command: string, options?: string, callback?: string) => void | string | Promise<unknown>;
    };
  }
}

const CONTROLLER = "/data/adb/modules/usb_samplerate_changer_webui/usbsrctl";

const pages = ["policy", "tools", "tuning"] as const;
type PageId = typeof pages[number];
const pageItems: Array<{ id: PageId; label: string; icon: typeof PolicyIcon }> = [
  { id: "policy", label: "策略", icon: PolicyIcon },
  { id: "tools", label: "工具", icon: ToolsIcon },
  { id: "tuning", label: "调优", icon: TuneIcon }
];

const policyOptions = [
  ["auto", "自动检测", "根据设备 HAL 和 Bluetooth 能力选择模板"],
  ["offload", "硬件卸载", "USB 与蓝牙硬件卸载"],
  ["offload-hifi-playback", "USB HiFi Playback", "USB hifi_playback 硬件卸载"],
  ["offload-direct", "Direct PCM", "使用 direct_pcm / compressed_offload"],
  ["offload-safer", "较安全卸载", "保留蓝牙策略，使用较安全的 USB 卸载"],
  ["bypass", "绕过硬件卸载", "绕过 USB 与蓝牙硬件卸载"],
  ["bypass-safer", "较安全绕过", "绕过卸载但保留传统内部输出设置"],
  ["legacy", "Legacy A2DP", "旧版 Bluetooth A2DP HAL"],
  ["safe", "Safe", "传统兼容设置"],
  ["safest", "Safest", "最大兼容性设置"],
  ["safest-auto", "Safest Auto", "兼容性设置并自动探测 USB 上限"],
  ["usb", "USB Only", "只修改 USB 音频策略"]
] as const;

const policyDetails: Record<string, string> = {
  auto: "自动检查设备的音频策略格式、USB HAL 与 Bluetooth 实现，再选择匹配模板。适合首次使用和 ROM 更新后的重新探测；实际使用的 XML 可能随设备环境变化。",
  offload: "同时保留 USB 与 Bluetooth 的硬件卸载路径，让受支持的音频流尽量绕过 AudioFlinger 混音。适合卸载链路完整的设备；HAL 或厂商路由不兼容时可能出现无声。",
  "offload-hifi-playback": "使用 hifi_playback 输出路径进行 USB 硬件卸载。只适合明确提供该输出 profile 的 ROM；没有对应 profile 时通常无法建立 USB 输出。",
  "offload-direct": "优先使用 direct_pcm / compressed_offload 一类直接输出路径，减少系统混音介入。适合 Qualcomm 等提供 Direct PCM 的设备，也是当前设备已验证的方案。",
  "offload-safer": "使用较保守的 USB 卸载配置，同时尽量保留原有 Bluetooth 与内部扬声器路由。适合标准 offload 会破坏其他输出设备时使用。",
  bypass: "绕过 USB 与 Bluetooth 的硬件卸载，让音频回到 AudioFlinger 混音和软件重采样路径。兼容性通常更高，但不再追求硬件直通。",
  "bypass-safer": "在绕过卸载的基础上保留更多传统内部输出与厂商路由定义。适合普通 bypass 导致蓝牙、扬声器或听筒异常的设备。",
  legacy: "使用旧版 A2DP / bluetooth_qti 风格策略。面向仍依赖传统 Bluetooth Audio HAL 的旧 ROM；现代 AIDL/HIDL 蓝牙栈通常不应优先选择。",
  safe: "采用保守的传统兼容配置，减少对厂商音频策略结构的改动。适合排查高级模板造成的路由问题。",
  safest: "进一步降低模板假设，只保留最通用的输出定义。适合恢复声音或确认问题是否来自厂商专有路径，功能和直通能力也最有限。",
  "safest-auto": "以最大兼容模板为基础，同时自动探测 USB 采样率上限。适合未知 DAC 或旧版策略格式，但探测结果仍受内核与 USB HAL 能力限制。",
  usb: "只修改 USB 音频相关策略，不主动改变 Bluetooth 和其他内部输出。适合仅需要外接 USB DAC、并希望最大限度降低系统其他路由影响的场景。"
};
const policySelectOptions = policyOptions.map(([value, label]) => [value, label] as const);

const rateOptions = [
  ["44100", "44.1 kHz"],
  ["48000", "48 kHz"],
  ["88200", "88.2 kHz"],
  ["96000", "96 kHz"],
  ["176400", "176.4 kHz"],
  ["192000", "192 kHz"],
  ["352800", "352.8 kHz"],
  ["384000", "384 kHz"],
  ["705600", "705.6 kHz"],
  ["768000", "768 kHz"]
] as const;

const bitOptions = [
  ["16", "16-bit PCM"],
  ["24", "24-bit packed PCM"],
  ["32", "32-bit PCM"],
  ["float", "32-bit float PCM"]
] as const;

const bluetoothHalOptions = [
  ["offload", "硬件卸载（推荐）"],
  ["aosp", "AOSP Bluetooth HAL"],
  ["legacy", "Legacy / bluetooth_qti HAL"],
  ["sysbta", "System Bluetooth Audio HAL"]
] as const;

const resamplerOptions = [
  ["default", "脚本默认 · 179 dB / 408 / 99 cheat"],
  ["159-480-92", "旧版默认 · 159 dB / 480 / 92"],
  ["165-360-104", "低性能设备 · 165 dB / 360 / 104"],
  ["179-408-99", "Android 12+ 推荐 · 179 dB / 408 / 99"],
  ["194-520-100", "1:1 Bit-perfect · 194 dB / 520 / 100"],
  ["ultra-hifi", "Ultra HiFi · 194 dB / 520 / 98 cheat"],
  ["cheap-44", "廉价 DAC 44.1 kHz · 194/520/92"],
  ["cheap-44-low", "廉价 DAC 44.1 kHz 低负载 · 194/520/91"],
  ["cheap-48", "廉价 DAC 48 kHz · 194/520/84"],
  ["cheap-48-low", "廉价 DAC 48 kHz 低负载 · 194/520/83"],
  ["cheap-96", "廉价 DAC 96 kHz · 194/520/42"],
  ["mock-dac-a", "Mock DAC-A · 150/80/109"],
  ["mock-dac-b", "Mock DAC-B · 120/80/97"],
  ["mock-dac-c", "Mock DAC-C · 100/80/104"],
  ["mock-mastering", "Mock Mastering · 159/240/99"]
] as const;

const usbPeriodOptions = Array.from({ length: 400 }, (_, index) => String((index + 1) * 125));
const diagnosticOptions = [
  ["audio", "Audio policy / AudioFlinger"],
  ["bluetooth", "Bluetooth codec 与 A2DP"],
  ["config", "音频配置文件探测"],
  ["alsa", "ALSA hw_params / DAC profile"]
] as const;

const rateSelectOptions = [...rateOptions, ["custom", "自定义整数（44100–768000 Hz）"]] as const;
const resamplerSelectOptions = [...resamplerOptions, ["custom", "自定义完整参数"]] as const;
const resamplerBypassOptions = [["none", "44.1 kHz 起"], ["48", "48 kHz 起"], ["96", "96 kHz 起"]] as const;
const ioSchedulerOptions = ["*", "none", "noop", "deadline", "mq-deadline", "cfq", "bfq", "kyber"].map((value) => [value, value === "*" ? "自动选择" : value] as const);
const ioToneOptions = ["light", "m-light", "medium", "boost", "exp"].map((value) => [value, value] as const);

const jitterFeatures = [
  ["selinux", "SELinux", "启用会切换到 Permissive，安全风险高"],
  ["thermal", "温控限制", "启用 reducer 会停止或弱化温控服务"],
  ["doze", "Doze", "调整省电与后台限制"],
  ["governor", "CPU/GPU Governor", "调整频率策略以减少调度抖动"],
  ["camera", "Camera 服务", "停止相机相关后台服务"],
  ["logd", "日志服务", "停止部分日志服务"],
  ["io", "I/O 调度", "调整 scheduler、read-ahead 与队列参数"],
  ["vm", "虚拟内存", "调整 swappiness 和脏页回写"],
  ["wifi", "Wi-Fi 省电", "此项的修改可能跨重启保留"],
  ["battery", "电池自适应", "调整自适应电池和充电管理"],
  ["effect", "音效框架", "禁用系统音效框架"]
] as const;

type JitterFeature = typeof jitterFeatures[number][0];
type JitterValues = Record<JitterFeature, boolean>;

const defaultJitterValues = (): JitterValues => Object.fromEntries(
  jitterFeatures.map(([key]) => [key, false])
) as JitterValues;

type Settings = {
  policy: string;
  rate: string;
  customRate: string;
  bitDepth: string;
  drc: boolean;
  forceUsbv2: boolean;
  forceBluetoothQti: boolean;
};

type Status = {
  controller_version?: string;
  script_version?: string;
  module_dir?: string;
  policy?: string;
  sample_rate?: string;
  bit_depth?: string;
  drc?: string;
  force_usbv2?: string;
  force_bluetooth_qti?: string;
  bluetooth_a2dp_connected?: string;
  amzm?: string;
  test?: string;
  test_template?: string;
  audioserver_pid?: string;
  self_ns?: string;
  audio_ns?: string;
  namespace_ok?: string;
  last_action?: string;
  last_route?: string;
  last_exit?: string;
  last_time?: string;
  templates?: string[];
};

const defaultSettings = (): Settings => ({
  policy: "auto",
  rate: "44100",
  customRate: "44100",
  bitDepth: "32",
  drc: false,
  forceUsbv2: false,
  forceBluetoothQti: false
});

let callbackSequence = 0;

function rootExec(command: string): Promise<ExecResult> {
  return new Promise((resolve, reject) => {
    const ksu = window.ksu;
    if (!ksu || typeof ksu.exec !== "function") {
      reject(new Error("未检测到 KernelSU/APatch WebUI root 接口"));
      return;
    }
    const id = `cb${Date.now().toString(36)}_${(++callbackSequence).toString(36)}`;
    const callbacks: Record<string, (code: number, stdout: string, stderr: string) => void> =
      window.__usbSrCallbacks ?? (window.__usbSrCallbacks = Object.create(null));
    const callbackRef = `window.__usbSrCallbacks.${id}`;
    const timeout = window.setTimeout(() => {
      delete callbacks[id];
      reject(new Error("root 命令执行超时"));
    }, 120000);
    callbacks[id] = (code, stdout, stderr) => {
      window.clearTimeout(timeout);
      delete callbacks[id];
      resolve({ code: Number(code), stdout: String(stdout || ""), stderr: String(stderr || "") });
    };
    try {
      void ksu.exec(command, "{}", callbackRef);
    } catch (error) {
      window.clearTimeout(timeout);
      delete callbacks[id];
      reject(error);
    }
  });
}

function shellQuote(value: string): string {
  return `'${value.replaceAll("'", "'\"'\"'")}'`;
}

function parseStatus(output: string): Status {
  const result: Status = {};
  const templates: string[] = [];
  let inTemplates = false;
  for (const line of output.split(/\r?\n/)) {
    if (line === "templates_begin") {
      inTemplates = true;
      continue;
    }
    if (line === "templates_end") {
      inTemplates = false;
      continue;
    }
    const separator = line.indexOf("=");
    if (separator < 1) continue;
    const key = line.slice(0, separator);
    const value = line.slice(separator + 1);
    if (inTemplates && key === "template") templates.push(value);
    else (result as Record<string, unknown>)[key] = value;
  }
  result.templates = templates;
  return result;
}

function rateValue(settings: Settings): string {
  return settings.rate === "custom" ? settings.customRate : settings.rate;
}

function controllerArgs(settings: Settings): string {
  const args = [
    "--policy", settings.policy,
    "--sample-rate", rateValue(settings),
    "--bit-depth", settings.bitDepth
  ];
  if (settings.drc) args.push("--drc");
  if (settings.forceUsbv2) args.push("--force-usbv2");
  if (settings.forceBluetoothQti) args.push("--force-bluetooth-qti");
  return args.map(shellQuote).join(" ");
}

function displayRate(value: string): string {
  const known = rateOptions.find(([rate]) => rate === value);
  if (known) return known[1];
  const numeric = Number(value);
  return Number.isFinite(numeric) ? `${numeric.toLocaleString()} Hz` : "未设置";
}

function initialFromStatus(status: Status): Settings {
  const settings = defaultSettings();
  if (status.policy) settings.policy = status.policy;
  if (status.sample_rate) {
    const known = rateOptions.some(([rate]) => rate === status.sample_rate);
    settings.rate = known ? status.sample_rate : "custom";
    settings.customRate = status.sample_rate;
  }
  if (status.bit_depth) settings.bitDepth = status.bit_depth;
  settings.drc = status.drc === "1";
  settings.forceUsbv2 = status.force_usbv2 === "1";
  settings.forceBluetoothQti = status.force_bluetooth_qti === "1";
  return settings;
}

function App() {
  const [settings, setSettings] = createSignal<Settings>(defaultSettings());
  const [status, setStatus] = createSignal<Status>({});
  const [busy, setBusy] = createSignal(false);
  const [toasts, setToasts] = createSignal<ToastItem[]>([]);
  const [log, setLog] = createSignal("");
  const [policyHelpOpen, setPolicyHelpOpen] = createSignal(false);
  const [bluetoothHal, setBluetoothHal] = createSignal("offload");
  const [resamplerPreset, setResamplerPreset] = createSignal("179-408-99");
  const [resamplerBypass, setResamplerBypass] = createSignal("none");
  const [resamplerCheat, setResamplerCheat] = createSignal(true);
  const [resamplerStopBand, setResamplerStopBand] = createSignal("179");
  const [resamplerHalfLength, setResamplerHalfLength] = createSignal("408");
  const [resamplerPercent, setResamplerPercent] = createSignal("99");
  const [usbPeriod, setUsbPeriod] = createSignal("2250");
  const [diagnostic, setDiagnostic] = createSignal("audio");
  const [diagnosticAll, setDiagnosticAll] = createSignal(false);
  const [diagnosticOutput, setDiagnosticOutput] = createSignal("尚未运行诊断。");
  const [jitterValues, setJitterValues] = createSignal<JitterValues>(defaultJitterValues());
  const [jitterDirty, setJitterDirty] = createSignal<JitterFeature[]>([]);
  const [ioScheduler, setIoScheduler] = createSignal("*");
  const [ioTone, setIoTone] = createSignal("medium");
  const [wifiNoRestart, setWifiNoRestart] = createSignal(false);
  const [activePage, setActivePage] = createSignal<PageId>("policy");
  const [pageProgress, setPageProgress] = createSignal(0);
  const [pageDragging, setPageDragging] = createSignal(false);
  const [logOpen, setLogOpen] = createSignal(false);

  let pageViewport: HTMLDivElement | undefined;
  let carousel: EmblaCarouselType | undefined;

  const activeIndex = createMemo(() => pages.indexOf(activePage()));

  function syncCarousel() {
    if (!carousel) return;
    const progress = Math.max(0, Math.min(1, carousel.scrollProgress()));
    setPageProgress(progress * (pages.length - 1));
    const nextPage = pages[carousel.selectedScrollSnap()] ?? "policy";
    if (nextPage === activePage()) return;
    setActivePage(nextPage);
    window.history.replaceState({ ...window.history.state, usbSrPage: nextPage }, "");
  }

  function activatePage(page: PageId) {
    const index = pages.indexOf(page);
    if (!carousel) {
      setActivePage(page);
      setPageProgress(index);
      return;
    }
    carousel.scrollTo(index);
  }

  function openOverlay(name: "policy-help" | "log") {
    window.history.pushState({ ...window.history.state, usbSrPage: activePage(), usbSrOverlay: name }, "");
    if (name === "policy-help") setPolicyHelpOpen(true);
    else setLogOpen(true);
  }

  function closeOverlay(name: "policy-help" | "log", fromHistory = false) {
    if (name === "policy-help") setPolicyHelpOpen(false);
    else setLogOpen(false);
    if (!fromHistory && window.history.state?.usbSrOverlay === name) window.history.back();
  }

  function showToast(message: string, tone?: "success" | "error") {
    const resolvedTone = tone ?? (
      message.toLowerCase().includes("error") || message.includes("失败")
        ? "error"
        : message.includes("已") || message.includes("完成") || message.includes("成功")
          ? "success"
          : undefined
    );
    const id = Math.random().toString(36).slice(2);
    setToasts((current) => [...current, { id, message, tone: resolvedTone }]);
    window.setTimeout(() => {
      setToasts((current) => current.filter((toast) => toast.id !== id));
    }, 4200);
  }

  const update = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((current) => ({ ...current, [key]: value }));
  };

  async function refresh(showSuccess = true) {
    setBusy(true);
    try {
      const result = await rootExec(`${CONTROLLER} status`);
      if (result.code !== 0) throw new Error(result.stderr || result.stdout || "读取状态失败");
      const parsed = parseStatus(result.stdout);
      setStatus(parsed);
      setSettings(initialFromStatus(parsed));
      if (showSuccess) showToast("状态已更新");
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function confirmApplyWithConnectedA2dp(): Promise<boolean> {
    const result = await rootExec(`${CONTROLLER} status`);
    if (result.code !== 0) throw new Error(result.stderr || result.stdout || "读取 A2DP 状态失败");
    const liveStatus = parseStatus(result.stdout);
    setStatus(liveStatus);
    if (liveStatus.bluetooth_a2dp_connected !== "1") return true;
    return window.confirm("当前检测到 A2DP 蓝牙音频设备已连接。应用音频修改会重启音频服务，可能导致蓝牙音频连接失效。是否继续应用？");
  }

  async function handleA2dpFailure(action: string) {
    const openSettings = window.confirm(`${action}已完成，但 A2DP 路由检查失败，蓝牙媒体音频可能已经失效。是否自动打开蓝牙设置进行重连？`);
    if (!openSettings) {
      showToast("A2DP 路由检查失败，请手动断开并重新连接蓝牙音频设备", "error");
      return;
    }
    const result = await rootExec("am start -a android.settings.BLUETOOTH_SETTINGS");
    if (result.code !== 0) throw new Error(result.stderr || result.stdout || "无法打开蓝牙设置");
    showToast("已打开蓝牙设置，请断开并重新连接音频设备");
  }

  async function apply() {
    const current = settings();
    const numericRate = Number(rateValue(current));
    if (!Number.isInteger(numericRate) || numericRate < 44100 || numericRate > 768000) {
      showToast("采样率必须是 44100–768000 Hz 的整数", "error");
      return;
    }
    setBusy(true);
    try {
      if (!await confirmApplyWithConnectedA2dp()) return;
      showToast("正在生成脚本并应用，audioserver 可能会短暂重启…");
      const command = `${CONTROLLER} apply ${controllerArgs(current)}`;
      const result = await rootExec(command);
      const text = `${result.stdout}${result.stderr ? `\n[stderr]\n${result.stderr}` : ""}`.trim();
      setLog(text || `exit=${result.code}`);
      if (result.code === 72) {
        await handleA2dpFailure("音频策略");
        await refresh(false);
        return;
      }
      if (result.code !== 0) throw new Error(text || `应用失败，退出码 ${result.code}`);
      showToast(result.stdout.includes("bluetooth_a2dp_before=1") ? "配置已应用，A2DP 路由正常" : "配置已应用");
      await refresh(false);
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function reset() {
    if (!window.confirm("重置会卸载生成的 audio policy bind mount，并重启音频服务。继续吗？")) return;
    setBusy(true);
    showToast("正在重置…");
    try {
      const result = await rootExec(`${CONTROLLER} reset`);
      const text = `${result.stdout}${result.stderr ? `\n[stderr]\n${result.stderr}` : ""}`.trim();
      setLog(text || `exit=${result.code}`);
      if (result.code === 72) {
        await handleA2dpFailure("重置");
        await refresh(false);
        return;
      }
      if (result.code !== 0) throw new Error(text || `重置失败，退出码 ${result.code}`);
      showToast("已重置");
      await refresh(false);
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function runExtra(args: string[], _pending: string, success: string, confirmText?: string) {
    if (confirmText && !window.confirm(confirmText)) return;
    setBusy(true);
    try {
      const command = `${CONTROLLER} extra ${args.map(shellQuote).join(" ")}`;
      const result = await rootExec(command);
      const text = `${result.stdout}${result.stderr ? `\n[stderr]\n${result.stderr}` : ""}`.trim();
      setLog(text || `exit=${result.code}`);
      if (result.code === 72) {
        await handleA2dpFailure("设置");
        return;
      }
      if (result.code !== 0) throw new Error(text || `执行失败，退出码 ${result.code}`);
      showToast(success);
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function runDiagnostic() {
    setBusy(true);
    setDiagnosticOutput("正在读取设备诊断信息…");
    try {
      const args = ["diagnose", diagnostic(), ...(diagnosticAll() ? ["all"] : [])];
      const result = await rootExec(`${CONTROLLER} extra ${args.map(shellQuote).join(" ")}`);
      const text = `${result.stdout}${result.stderr ? `\n[stderr]\n${result.stderr}` : ""}`.trim();
      setDiagnosticOutput(text || `exit=${result.code}`);
      if (result.code !== 0) throw new Error(`诊断失败，退出码 ${result.code}`);
    } catch (error) {
      setDiagnosticOutput((current) => current === "正在读取设备诊断信息…" ? String(error) : current);
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  function updateJitter(feature: JitterFeature, value: boolean) {
    setJitterValues((current) => ({ ...current, [feature]: value }));
    setJitterDirty((current) => current.includes(feature) ? current : [...current, feature]);
  }

  async function applyJitter() {
    const dirty = jitterDirty();
    if (!dirty.length) {
      showToast("没有待应用的 jitter reducer 修改");
      return;
    }
    if ((dirty.includes("selinux") && jitterValues().selinux) || (dirty.includes("thermal") && jitterValues().thermal)) {
      if (!window.confirm("将关闭 SELinux enforcing 或系统温控保护，可能降低安全性并导致过热。仍要继续吗？")) return;
    }
    setBusy(true);
    const outputs: string[] = [];
    try {
      for (const feature of dirty) {
        const args = ["jitter", jitterValues()[feature] ? "enable" : "disable", feature];
        if (feature === "io" && jitterValues().io) args.push(ioScheduler(), ioTone());
        if (feature === "wifi" && jitterValues().wifi && wifiNoRestart()) args.push("no-restart");
        const result = await rootExec(`${CONTROLLER} extra ${args.map(shellQuote).join(" ")}`);
        outputs.push(`[${feature}]\n${result.stdout}${result.stderr}`.trim());
        if (result.code !== 0) throw new Error(`jitter ${feature} 执行失败，退出码 ${result.code}`);
      }
      setLog(outputs.join("\n\n"));
      setJitterDirty([]);
      showToast("Jitter reducer 设置已应用");
    } catch (error) {
      setLog(outputs.join("\n\n"));
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  onMount(() => {
    window.history.replaceState({ ...window.history.state, usbSrPage: activePage() }, "");
    const handlePopState = () => {
      if (policyHelpOpen()) {
        closeOverlay("policy-help", true);
        return;
      }
      if (logOpen()) {
        closeOverlay("log", true);
        return;
      }
    };
    window.addEventListener("popstate", handlePopState);
    onCleanup(() => window.removeEventListener("popstate", handlePopState));

    if (pageViewport) {
      carousel = EmblaCarousel(pageViewport, {
        align: "start",
        containScroll: "trimSnaps",
        duration: 18,
        loop: false,
        skipSnaps: false,
        watchDrag: (_api, event) => !(
          event.target instanceof Element && event.target.closest("[data-no-page-drag]")
        )
      });
      carousel.on("scroll", syncCarousel);
      carousel.on("select", syncCarousel);
      carousel.on("pointerDown", () => setPageDragging(true));
      carousel.on("pointerUp", () => setPageDragging(false));
      carousel.on("settle", () => {
        setPageDragging(false);
        syncCarousel();
      });
      syncCarousel();
    }
    void refresh(false);
  });

  onCleanup(() => carousel?.destroy());

  return (
    <>
      <div class="app-shell">
        <header class="app-header">
          <div class="brand">
            <span class="brand-mark"><BrandIcon /></span>
            <span class="brand-copy"><strong>SampleRate Changer</strong><small>Root audio policy</small></span>
          </div>
          <button class="icon-button" aria-label="刷新状态" title="刷新状态" onClick={() => void refresh()} disabled={busy()}><RefreshIcon /></button>
          <button class="icon-button" aria-label="打开执行日志" title="执行日志" onClick={() => openOverlay("log")}><LogIcon /></button>
        </header>

        <div class="page-viewport" ref={pageViewport}>
          <div class="page-track">
            <section class="page-panel" aria-label="音频策略页面">
              <main class="page-content">
                <section class="card preview-card">
                  <div class="section-heading"><h2>本次执行</h2></div>
                  <div class="summary-line"><span class="summary-key">策略</span><strong>{policyOptions.find(([value]) => value === settings().policy)?.[1]}</strong><span class="summary-key">格式</span><strong>{displayRate(rateValue(settings()))} · {bitOptions.find(([value]) => value === settings().bitDepth)?.[1]}</strong></div>
                  <div class="tag-row"><Show when={settings().drc}><span class="tag">DRC</span></Show><Show when={settings().forceUsbv2}><span class="tag">USBv2</span></Show><Show when={settings().forceBluetoothQti}><span class="tag">Bluetooth QTI</span></Show></div>
                  <div class="preview-actions"><button class="secondary-button" onClick={reset} disabled={busy()}>重置修改</button><button class="primary-button" onClick={apply} disabled={busy()}>{busy() ? "处理中…" : "应用"}</button></div>
                </section>

                <section class="grid two-col">
                  <article class="card section-card">
                    <SectionHeading eyebrow="POLICY MODE" title="音频策略" number="01" />
                    <div class="field-label-row"><label class="field-label" for="policy">策略模板</label><button type="button" class="inline-link" onClick={() => openOverlay("policy-help")}>模板说明</button></div>
                    <SelectField id="policy" title="选择策略模板" value={settings().policy} options={policySelectOptions} onChange={(value) => update("policy", value)} />
                    <p class="field-help">{policyOptions.find(([value]) => value === settings().policy)?.[2]}</p>
                  </article>

                  <article class="card section-card">
                    <SectionHeading eyebrow="FORMAT" title="采样格式" number="02" />
                    <label class="field-label" for="rate">采样率</label>
                    <SelectField id="rate" title="选择采样率" value={settings().rate} options={rateSelectOptions} onChange={(value) => update("rate", value)} />
                    <Show when={settings().rate === "custom"}><input class="number-input spaced-input" inputmode="numeric" type="number" min="44100" max="768000" step="1" value={settings().customRate} onInput={(event) => update("customRate", event.currentTarget.value)} placeholder="例如 123456" /></Show>
                    <label class="field-label" for="bits">位深 / 格式</label>
                    <SelectField id="bits" title="选择位深与格式" value={settings().bitDepth} options={bitOptions} onChange={(value) => update("bitDepth", value)} />
                  </article>
                </section>

                <section class="card section-card">
                  <SectionHeading eyebrow="SWITCHES" title="功能开关" number="03" />
                  <div class="switch-grid">
                    <ToggleRow label="DRC 动态范围控制" description="USB-only 模式不生效。" checked={settings().drc} onChange={(value) => update("drc", value)} />
                    <ToggleRow label="强制 USBv2 HAL" description="优先使用 usbv2 HAL。" checked={settings().forceUsbv2} onChange={(value) => update("forceUsbv2", value)} />
                    <ToggleRow label="强制 Bluetooth QTI" description="强制 Qualcomm bluetooth_qti HAL。" checked={settings().forceBluetoothQti} onChange={(value) => update("forceBluetoothQti", value)} />
                  </div>
                </section>

                <section class="status-strip card">
                  <div><span class="label">audioserver</span><strong>{status().audioserver_pid || "未检测到"}</strong></div>
                  <div><span class="label">脚本版本</span><strong>{status().script_version || "—"}</strong></div>
                  <div><span class="label">当前采样率</span><strong>{status().sample_rate ? displayRate(status().sample_rate ?? "") : "—"}</strong></div>
                  <div><span class="label">Bluetooth A2DP</span><strong>{status().bluetooth_a2dp_connected === "1" ? "已连接" : "未连接"}</strong></div>
                </section>
              </main>
            </section>

            <section class="page-panel" aria-label="扩展工具页面">
              <main class="page-content">
                <div class="page-intro"><div class="intro-icon"><ToolsIcon /></div><div><p class="eyebrow">EXTRAS</p><h1>音频扩展工具</h1></div></div>
                <section class="grid two-col extra-grid">
                  <article class="card tool-panel">
                    <SectionHeading eyebrow="BLUETOOTH" title="Bluetooth HAL" number="01" />
                    <p class="field-help">切换实现会修改持久属性并重启音频 HAL。</p>
                    <SelectField title="选择 Bluetooth HAL" value={bluetoothHal()} options={bluetoothHalOptions} onChange={(value) => setBluetoothHal(value)} />
                    <div class="inline-actions"><button class="primary-button" disabled={busy()} onClick={() => runExtra(["bluetooth-hal", bluetoothHal()], "正在切换 Bluetooth HAL…", "Bluetooth HAL 已切换")}>应用</button></div>
                  </article>

                  <article class="card tool-panel">
                    <SectionHeading eyebrow="RESAMPLER" title="AudioFlinger 重采样" number="02" />
                    <p class="field-help">设置 stop-band、滤波器长度和截止频率预设。</p>
                    <SelectField title="选择重采样预设" value={resamplerPreset()} options={resamplerSelectOptions} onChange={(value) => setResamplerPreset(value)} />
                    <Show when={resamplerPreset() === "custom"}><div class="custom-resampler">
                      <label class="field-label">生效起始采样率</label><SelectField title="选择生效起始采样率" value={resamplerBypass()} options={resamplerBypassOptions} onChange={(value) => setResamplerBypass(value)} />
                      <ToggleRow label="Cheat 模式" description="关闭时使用标准 cutoff_percent。" checked={resamplerCheat()} onChange={setResamplerCheat} />
                      <label class="field-label">Stop band（20–242 dB）</label><SelectField title="选择 Stop band" value={resamplerStopBand()} options={Array.from({ length: 223 }, (_, index) => { const value = String(index + 20); return [value, `${value} dB`] as const; })} onChange={(value) => setResamplerStopBand(value)} />
                      <label class="field-label">Half filter length（8–640）</label><SelectField title="选择 Half filter length" value={resamplerHalfLength()} options={Array.from({ length: 80 }, (_, index) => { const value = String((index + 1) * 8); return [value, value] as const; })} onChange={(value) => setResamplerHalfLength(value)} />
                      <label class="field-label">{resamplerCheat() ? "Cheat" : "Cutoff"} 百分比</label><SelectField title={`选择 ${resamplerCheat() ? "Cheat" : "Cutoff"} 百分比`} value={resamplerPercent()} options={Array.from({ length: resamplerCheat() ? 200 : 100 }, (_, index) => { const value = String(index + 1); return [value, `${value}%`] as const; })} onChange={(value) => setResamplerPercent(value)} />
                    </div></Show>
                    <div class="inline-actions"><button class="secondary-button" disabled={busy()} onClick={() => runExtra(["resampler", "reset"], "正在重置重采样…", "重采样已恢复系统默认", "确定重置 AudioFlinger 重采样属性吗？")}>重置</button><button class="primary-button" disabled={busy()} onClick={() => runExtra(resamplerPreset() === "custom" ? ["resampler", "custom", resamplerBypass(), resamplerCheat() ? "cheat" : "cutoff", resamplerStopBand(), resamplerHalfLength(), resamplerPercent()] : ["resampler", resamplerPreset()], "正在应用重采样配置…", "重采样配置已应用")}>应用</button></div>
                  </article>

                  <article class="card tool-panel">
                    <SectionHeading eyebrow="USB TIMING" title="USB Transfer Period" number="03" />
                    <p class="field-help">125–50000 μs，使用项目支持的 125 μs 步进。</p>
                    <SelectField title="选择 USB Transfer Period" value={usbPeriod()} options={usbPeriodOptions.map((period) => [period, `${period} μs`] as const)} onChange={(value) => setUsbPeriod(value)} />
                    <div class="inline-actions"><button class="secondary-button" disabled={busy()} onClick={() => runExtra(["usb-period", "reset"], "正在重置 USB period…", "USB period 已恢复系统默认")}>重置</button><button class="primary-button" disabled={busy()} onClick={() => runExtra(["usb-period", usbPeriod()], "正在应用 USB period…", "USB period 已应用")}>应用</button></div>
                  </article>

                  <article class="card tool-panel">
                    <SectionHeading eyebrow="DIAGNOSTICS" title="诊断输出" number="04" />
                    <p class="field-help">读取 Audio、Bluetooth、配置文件和 ALSA 运行状态。</p>
                    <SelectField title="选择诊断类型" value={diagnostic()} options={diagnosticOptions} onChange={(value) => setDiagnostic(value)} />
                    <ToggleRow label="完整输出" description="关闭时只保留关键字段。" checked={diagnosticAll()} onChange={setDiagnosticAll} />
                    <div class="inline-actions"><button class="primary-button" disabled={busy()} onClick={runDiagnostic}>运行诊断</button></div>
                    <pre class="diagnostic-output" aria-live="polite">{diagnosticOutput()}</pre>
                  </article>
                </section>
              </main>
            </section>

            <section class="page-panel" aria-label="系统调优页面">
              <main class="page-content">
                <div class="page-intro danger-intro"><div class="intro-icon"><TuneIcon /></div><div><p class="eyebrow">JITTER REDUCER</p><h1>系统抖动调节</h1></div></div>
                <section class="card section-card danger-card">
                  <div class="notice danger-notice"><WarningIcon /><span><strong>风险提示：</strong>这些选项可能修改 SELinux、温控、系统服务、调度器和内核参数，可能降低系统安全性、稳定性或导致设备过热。请仅启用已了解影响的项目；SELinux 与温控选项应用时会再次确认。</span></div>
                  <div class="switch-grid jitter-grid"><For each={jitterFeatures}>{(feature) => <ToggleRow label={`${feature[1]}${jitterDirty().includes(feature[0]) ? " · 待应用" : ""}`} description={feature[2]} checked={jitterValues()[feature[0]]} onChange={(value) => updateJitter(feature[0], value)} />}</For></div>
                  <Show when={jitterValues().io && jitterDirty().includes("io")}><div class="grid two-col io-options"><div><label class="field-label">I/O scheduler</label><SelectField title="选择 I/O scheduler" value={ioScheduler()} options={ioSchedulerOptions} onChange={(value) => setIoScheduler(value)} /></div><div><label class="field-label">声音倾向</label><SelectField title="选择声音倾向" value={ioTone()} options={ioToneOptions} onChange={(value) => setIoTone(value)} /></div></div></Show>
                  <Show when={jitterValues().wifi && jitterDirty().includes("wifi")}><div class="wifi-option"><ToggleRow label="切换时不重启 Wi-Fi" description="使用 upstream 的 no-restart 模式。" checked={wifiNoRestart()} onChange={setWifiNoRestart} /></div></Show>
                  <div class="inline-actions right"><button class="secondary-button" disabled={busy()} onClick={() => runExtra(["jitter", "disable", "all"], "正在恢复全部基础 jitter 项…", "基础 jitter 项已恢复")}>恢复基础项</button><button class="primary-button" disabled={busy() || !jitterDirty().length} onClick={applyJitter}>应用 {jitterDirty().length || ""} 项修改</button></div>
                </section>

              </main>
            </section>
          </div>
        </div>

        <nav
          class="page-navigation"
          classList={{ dragging: pageDragging() }}
          style={{ "--page-progress": pageProgress() }}
          aria-label="主要页面"
        >
          <span class="page-nav-indicator" aria-hidden="true" />
          <For each={pageItems}>{(item, index) => {
            const PageIcon = item.icon;
            return <button class="page-nav-item" classList={{ active: activePage() === item.id }} aria-current={activeIndex() === index() ? "page" : undefined} onClick={() => activatePage(item.id)}><PageIcon /><span>{item.label}</span></button>;
          }}</For>
        </nav>
      </div>

      <ToastHost toasts={toasts()} />

      <Show when={policyHelpOpen()}>
        <div class="dialog-layer" data-no-page-drag role="presentation">
          <button class="dialog-backdrop" aria-label="关闭模板说明" onClick={() => closeOverlay("policy-help")} />
          <section class="info-dialog" role="dialog" aria-modal="true" aria-labelledby="policy-help-title">
            <header><div><h2 id="policy-help-title">策略模板说明</h2><p>模板决定生成 audio policy XML 时保留或绕过哪些输出路径；最终结果仍受 ROM、HAL 和设备能力影响。</p></div><button class="dialog-close" aria-label="关闭模板说明" onClick={() => closeOverlay("policy-help")}>×</button></header>
            <div class="policy-guide-list">
              <For each={policyOptions}>{([value, label, summary]) => <article classList={{ current: value === settings().policy }}><div><h3>{label}</h3><Show when={value === settings().policy}><span>当前选择</span></Show></div><p class="policy-summary">{summary}</p><p>{policyDetails[value]}</p></article>}</For>
            </div>
          </section>
        </div>
      </Show>

      <Show when={logOpen()}>
        <div class="dialog-layer" role="presentation">
          <button class="dialog-backdrop" aria-label="关闭执行日志" onClick={() => closeOverlay("log")} />
          <section class="log-dialog" role="dialog" aria-modal="true" aria-labelledby="log-title">
            <header><div><p class="eyebrow">LAST OUTPUT</p><h2 id="log-title">执行日志</h2></div><button class="icon-button" aria-label="关闭执行日志" onClick={() => closeOverlay("log")}>×</button></header>
            <pre>{log() || "暂无执行输出。运行状态查询、诊断或应用配置后会显示在这里。"}</pre>
          </section>
        </div>
      </Show>
    </>
  );
}

function ToastHost(props: { toasts: ToastItem[] }) {
  return <div id="toast-container" aria-live="polite"><For each={props.toasts}>{(toast) => <div class={`toast${toast.tone ? ` ${toast.tone}` : ""}`}>{toast.message}</div>}</For></div>;
}

function SectionHeading(props: { eyebrow: string; title: string; number: string }) {
  return <div class="section-heading"><h2>{props.title}</h2></div>;
}

function ToggleRow(props: { label: string; description: string; checked: boolean; disabled?: boolean; onChange: (value: boolean) => void }) {
  return <label class={`switch-row ${props.disabled ? "disabled" : ""}`}><span class="switch-copy"><strong>{props.label}</strong><small>{props.description}</small></span><input type="checkbox" checked={props.checked} disabled={props.disabled} onChange={(event) => props.onChange(event.currentTarget.checked)} /><span class="switch-track"><span /></span></label>;
}

render(() => <App />, document.getElementById("app")!);
