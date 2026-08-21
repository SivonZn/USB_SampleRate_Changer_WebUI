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
import { type Language, translate, translateRuntime } from "./i18n";
import {
  BrandIcon,
  LogIcon,
  PolicyIcon,
  RefreshIcon,
  SettingsIcon,
  ToolsIcon,
  TuneIcon,
  WarningIcon
} from "./Icons";

declare const __WEBUI_VERSION__: string;

type ExecResult = { code: number; stdout: string; stderr: string };
type ToastItem = { id: string; message: string; tone?: "success" | "error" };
type ConfirmRequest = {
  title: string;
  message: string;
  confirmLabel: string;
  action: () => void;
  cancel?: () => void;
  showCancel?: boolean;
};

declare global {
  interface Window {
    __usbSrCallbacks?: Record<string, (code: number, stdout: string, stderr: string) => void>;
    ksu?: {
      exec?: (command: string, options?: string, callback?: string) => void | string | Promise<unknown>;
    };
  }
}

const CONTROLLER = "/data/adb/modules/usb_samplerate_changer_webui/usbsrctl";
const WEBUI_VERSION = typeof __WEBUI_VERSION__ === "string" ? __WEBUI_VERSION__ : "dev";

const pages = ["policy", "tools", "tuning", "settings"] as const;
type PageId = typeof pages[number];
const pageItems: Array<{ id: PageId; label: string; icon: typeof PolicyIcon }> = [
  { id: "policy", label: "策略", icon: PolicyIcon },
  { id: "tools", label: "工具", icon: ToolsIcon },
  { id: "tuning", label: "调优", icon: TuneIcon },
  { id: "settings", label: "设置", icon: SettingsIcon }
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
  "offload-direct": "优先使用 direct_pcm / compressed_offload 一类直接输出路径，减少系统混音介入。适合 Qualcomm 等提供 Direct PCM 的设备。",
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

const diagnosticOptions = [
  ["audio", "Audio policy / AudioFlinger"],
  ["bluetooth", "Bluetooth codec 与 A2DP"],
  ["config", "音频配置文件探测"],
  ["alsa", "ALSA hw_params / DAC profile"]
] as const;
const languageOptions = [["zh-CN", "中文"], ["en", "English"]] as const;

const rateSelectOptions = [...rateOptions, ["custom", "自定义整数（44100–768000 Hz）"]] as const;
const resamplerSelectOptions = [...resamplerOptions, ["custom", "自定义完整参数"]] as const;
const resamplerBypassOptions = [["none", "44.1 kHz 起"], ["48", "48 kHz 起"], ["96", "96 kHz 起"]] as const;
const ioSchedulerOptions = ["*", "none", "noop", "deadline", "mq-deadline", "cfq", "bfq", "kyber"].map((value) => [value, value === "*" ? "自动选择" : value] as const);
const ioToneOptions = ["light", "m-light", "medium", "boost", "exp"].map((value) => [value, value] as const);

const jitterFeatures = [
  ["selinux", "允许 SELinux Permissive", "打开后允许系统进入 Permissive；关闭并应用则恢复 Enforcing。安全风险高。"],
  ["thermal", "停用温控", "打开后停止或弱化温控服务；关闭并应用则尝试恢复系统温控。"],
  ["doze", "停用 Doze", "打开后停用设备空闲省电机制，减少后台唤醒带来的调度变化。"],
  ["governor", "锁定高性能调频", "打开后将支持的 CPU/GPU 调频策略设为高性能，减少频率切换。"],
  ["camera", "停用相机服务", "打开后停止相机后台服务；需要使用相机时请关闭并应用。"],
  ["logd", "停用日志服务", "打开后停止 logd 与部分追踪服务，减少持续日志写入。"],
  ["io", "优化 I/O 调度", "打开后调整调度器、预读和队列参数，降低存储访问干扰。"],
  ["vm", "优化虚拟内存", "打开后调整 swappiness 与脏页回写策略，减少突发写入。"],
  ["wifi", "停用 Wi-Fi 省电", "打开后停用 Wi-Fi 挂起优化和自适应连接；部分修改可能跨重启保留。"],
  ["battery", "停用自适应电池", "打开后停用自适应电池与自适应充电管理，减少后台干预。"],
  ["effect", "停用系统音效", "打开后绕过系统音效框架及相关服务，减少额外音频后处理。"]
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
  const [confirmRequest, setConfirmRequest] = createSignal<ConfirmRequest>();
  const [language, setLanguage] = createSignal<Language>(() => {
    const stored = window.localStorage.getItem("usbSrLanguage");
    return stored === "en" ? "en" : "zh-CN";
  });

  const tx = (value: string) => translate(language(), value);
  const localizeOptions = (options: ReadonlyArray<readonly [string, string]>) =>
    options.map(([value, label]) => [value, tx(label)] as const);
  const localizePolicyOptions = () => policyOptions.map(([value, label]) => [value, tx(label)] as const);

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

  function openConfirm(request: ConfirmRequest) {
    window.history.pushState({ ...window.history.state, usbSrPage: activePage(), usbSrOverlay: "confirm" }, "");
    setConfirmRequest({
      ...request,
      title: tx(request.title),
      message: translateRuntime(language(), request.message),
      confirmLabel: tx(request.confirmLabel)
    });
  }

  function closeConfirm(fromHistory = false) {
    setConfirmRequest(undefined);
    if (!fromHistory && window.history.state?.usbSrOverlay === "confirm") window.history.back();
  }

  function dismissConfirm(fromHistory = false) {
    const cancel = confirmRequest()?.cancel;
    closeConfirm(fromHistory);
    cancel?.();
  }

  function acceptConfirm() {
    const action = confirmRequest()?.action;
    closeConfirm();
    action?.();
  }

  function confirmWebUi(title: string, message: string, confirmLabel = "继续"): Promise<boolean> {
    return new Promise((resolve) => {
      openConfirm({
        title,
        message,
        confirmLabel,
        action: () => resolve(true),
        cancel: () => resolve(false)
      });
    });
  }

  function showToast(message: string, tone?: "success" | "error") {
    const sourceMessage = message;
    message = translateRuntime(language(), message);
    const resolvedTone = tone ?? (
      sourceMessage.toLowerCase().includes("error") || sourceMessage.includes("失败")
        ? "error"
        : sourceMessage.includes("已") || sourceMessage.includes("完成") || sourceMessage.includes("成功")
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
      if (result.code !== 0) throw new Error(result.stderr || result.stdout || tx("读取状态失败"));
      const parsed = parseStatus(result.stdout);
      setStatus(parsed);
      setSettings(initialFromStatus(parsed));
      if (showSuccess) showToast(tx("状态已更新"));
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function confirmApplyWithConnectedA2dp(): Promise<boolean> {
    const result = await rootExec(`${CONTROLLER} status`);
    if (result.code !== 0) throw new Error(result.stderr || result.stdout || tx("读取 A2DP 状态失败"));
    const liveStatus = parseStatus(result.stdout);
    setStatus(liveStatus);
    if (liveStatus.bluetooth_a2dp_connected !== "1") return true;
    return confirmWebUi(
      "蓝牙音频正在使用",
      "当前检测到 A2DP 蓝牙音频设备已连接，继续应用可能导致当前蓝牙音频连接失效；若失效，需要先恢复可用策略模板，并断开重新连接蓝牙设备才可以正常恢复蓝牙音频。",
      "继续应用"
    );
  }

  async function handleA2dpFailure(action: string) {
    const openSettings = await confirmWebUi(
      "A2DP 路由异常",
      `${action}已完成，但 A2DP 路由检查失败，蓝牙媒体音频可能已经失效。需要打开蓝牙设置以断开并重新连接设备。`,
      "打开蓝牙设置"
    );
    if (!openSettings) {
      showToast("A2DP 路由检查失败，请手动断开并重新连接蓝牙音频设备", "error");
      return;
    }
    const result = await rootExec("am start -a android.settings.BLUETOOTH_SETTINGS");
    if (result.code !== 0) throw new Error(result.stderr || result.stdout || tx("无法打开蓝牙设置"));
    showToast(tx("已打开蓝牙设置，请断开并重新连接音频设备"));
  }

  async function apply() {
    const current = settings();
    const numericRate = Number(rateValue(current));
    if (!Number.isInteger(numericRate) || numericRate < 44100 || numericRate > 768000) {
      showToast(tx("采样率必须是 44100–768000 Hz 的整数"), "error");
      return;
    }
    setBusy(true);
    try {
      if (!await confirmApplyWithConnectedA2dp()) return;
      showToast(tx("正在应用更改，音频将会短暂断开"));
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
      showToast(result.stdout.includes("bluetooth_a2dp_before=1") ? tx("配置已应用，A2DP 路由正常") : tx("配置已应用"));
      await refresh(false);
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function reset() {
    setBusy(true);
    showToast(tx("正在重置…"));
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
      showToast(tx("已重置"));
      await refresh(false);
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function runExtra(args: string[], _pending: string, success: string) {
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
      showToast(tx(success));
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  }

  function normalizeUsbPeriod(value = usbPeriod()): string {
    const numeric = Number(value);
    if (!Number.isFinite(numeric)) return "2250";
    return String(Math.min(50000, Math.max(125, Math.round(numeric / 125) * 125)));
  }

  function stepUsbPeriod(direction: -1 | 1) {
    setUsbPeriod(normalizeUsbPeriod(String(Number(normalizeUsbPeriod()) + direction * 125)));
  }

  function applyUsbPeriod() {
    const normalized = normalizeUsbPeriod();
    if (normalized !== usbPeriod()) setUsbPeriod(normalized);
    void runExtra(["usb-period", normalized], "正在应用 USB period…", "USB period 已应用");
  }

  function changeRate(value: string) {
    update("rate", value);
    if (value !== "custom") return;
    window.setTimeout(() => openConfirm({
      title: "自定义采样率提示",
      message: "自定义采样率可能不受当前 DAC、USB Audio HAL 或音频策略支持。应用后如出现无声、失真或播放失败，请改用设备明确支持的采样率。",
      confirmLabel: "知道了",
      action: () => undefined,
      showCancel: false
    }), 220);
  }

  function applyLanguage() {
    window.localStorage.setItem("usbSrLanguage", language());
  }

  async function runDiagnostic() {
    setBusy(true);
    setDiagnosticOutput(tx("正在读取设备诊断信息…"));
    try {
      const args = ["diagnose", diagnostic(), ...(diagnosticAll() ? ["all"] : [])];
      const result = await rootExec(`${CONTROLLER} extra ${args.map(shellQuote).join(" ")}`);
      const text = `${result.stdout}${result.stderr ? `\n[stderr]\n${result.stderr}` : ""}`.trim();
      setDiagnosticOutput(text || `exit=${result.code}`);
      if (result.code !== 0) throw new Error(`诊断失败，退出码 ${result.code}`);
    } catch (error) {
      setDiagnosticOutput((current) => current === tx("正在读取设备诊断信息…") ? translateRuntime(language(), String(error)) : current);
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
      showToast(tx("没有待应用的 jitter reducer 修改"));
      return;
    }
    if ((dirty.includes("selinux") && jitterValues().selinux) || (dirty.includes("thermal") && jitterValues().thermal)) {
      const riskActions = [
        dirty.includes("selinux") && jitterValues().selinux ? tx("允许 SELinux Permissive") : "",
        dirty.includes("thermal") && jitterValues().thermal ? tx("停用系统温控") : ""
      ].filter(Boolean).join(language() === "en" ? ", and " : "，并");
      if (!await confirmWebUi(
        "确认高风险操作",
        language() === "en"
          ? `This will ${riskActions}. It may reduce system security and stability and could cause overheating or damage. Confirm that you understand the risk.`
          : `即将${riskActions}。这可能降低系统安全性和稳定性，并可能导致设备过热或损坏。请确认已了解风险。`,
        "接受风险并应用"
      )) return;
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
        if (result.code !== 0) throw new Error(`${feature} 执行失败，退出码 ${result.code}`);
      }
      setLog(outputs.join("\n\n"));
      setJitterDirty([]);
      showToast(tx("Jitter reducer 设置已应用"));
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
      if (confirmRequest()) {
        dismissConfirm(true);
        return;
      }
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
            <span class="brand-copy"><strong>SampleRate Changer</strong><small>{tx("Root audio policy")}</small></span>
          </div>
          <button class="icon-button" aria-label={tx("刷新状态")} title={tx("刷新状态")} onClick={() => void refresh()} disabled={busy()}><RefreshIcon /></button>
          <button class="icon-button" aria-label={tx("打开执行日志")} title={tx("执行日志")} onClick={() => openOverlay("log")}><LogIcon /></button>
        </header>

        <div class="page-viewport" ref={pageViewport}>
          <div class="page-track">
            <section class="page-panel" aria-label={tx("音频策略页面")}>
              <main class="page-content">
                <section class="card preview-card">
                  <div class="section-heading"><h2>{tx("本次执行")}</h2></div>
                  <div class="summary-line"><span class="summary-key">{tx("策略")}</span><strong>{tx(policyOptions.find(([value]) => value === settings().policy)?.[1] ?? "")}</strong><span class="summary-key">{tx("格式")}</span><strong>{tx(displayRate(rateValue(settings())))} · {tx(bitOptions.find(([value]) => value === settings().bitDepth)?.[1] ?? "")}</strong></div>
                  <div class="tag-row"><Show when={settings().drc}><span class="tag">DRC</span></Show><Show when={settings().forceUsbv2}><span class="tag">USBv2</span></Show><Show when={settings().forceBluetoothQti}><span class="tag">Bluetooth QTI</span></Show></div>
                  <div class="preview-actions"><button class="secondary-button" onClick={() => openConfirm({ title: "重置音频策略", message: "将卸载生成的 audio policy bind mount，并重启音频服务。当前音频连接可能会短暂中断。", confirmLabel: "确认重置", action: () => void reset() })} disabled={busy()}>{tx("重置修改")}</button><button class="primary-button" onClick={apply} disabled={busy()}>{busy() ? tx("处理中…") : tx("应用")}</button></div>
                </section>

                <section class="grid two-col">
                  <article class="card section-card">
                    <SectionHeading title={tx("音频策略")} />
                    <div class="field-label-row"><label class="field-label" for="policy">{tx("策略模板")}</label><button type="button" class="inline-link" onClick={() => openOverlay("policy-help")}>ⓘ {tx("说明")}</button></div>
                    <SelectField id="policy" title={tx("选择策略模板")} value={settings().policy} options={localizePolicyOptions()} language={language()} onChange={(value) => update("policy", value)} />
                  </article>

                  <article class="card section-card">
                    <SectionHeading title={tx("采样格式")} />
                    <label class="field-label" for="rate">{tx("采样率")}</label>
                    <SelectField id="rate" title={tx("选择采样率")} value={settings().rate} options={localizeOptions(rateSelectOptions)} language={language()} onChange={changeRate} />
                    <Show when={settings().rate === "custom"}><input class="number-input spaced-input" inputmode="numeric" type="number" min="44100" max="768000" step="1" value={settings().customRate} onInput={(event) => update("customRate", event.currentTarget.value)} placeholder={tx("例如 123456")} /></Show>
                    <label class="field-label" for="bits">{tx("位深 / 格式")}</label>
                    <SelectField id="bits" title={tx("选择位深与格式")} value={settings().bitDepth} options={localizeOptions(bitOptions)} language={language()} onChange={(value) => update("bitDepth", value)} />
                  </article>
                </section>

                <section class="card section-card">
                  <SectionHeading title={tx("功能开关")} />
                  <div class="switch-grid">
                    <ToggleRow label={tx("DRC 动态范围控制")} description={tx("压缩过大的音量动态；USB Only 策略下不会生效。")} checked={settings().drc} onChange={(value) => update("drc", value)} />
                    <ToggleRow label={tx("强制 USBv2 HAL")} description={tx("优先使用 USB Audio HAL v2，仅建议用于兼容性排查。")} checked={settings().forceUsbv2} onChange={(value) => update("forceUsbv2", value)} />
                    <ToggleRow label={tx("强制 Bluetooth QTI")} description={tx("强制使用 Qualcomm bluetooth_qti 路径，非高通设备请勿启用。")} checked={settings().forceBluetoothQti} onChange={(value) => update("forceBluetoothQti", value)} />
                  </div>
                </section>

                <section class="status-strip card">
                  <div><span class="label">audioserver</span><strong>{status().audioserver_pid || tx("未检测到")}</strong></div>
                  <div><span class="label">{tx("脚本版本")}</span><strong>{status().script_version || "—"}</strong></div>
                  <div><span class="label">{tx("当前采样率")}</span><strong>{status().sample_rate ? tx(displayRate(status().sample_rate ?? "")) : "—"}</strong></div>
                  <div><span class="label">Bluetooth A2DP</span><strong>{status().bluetooth_a2dp_connected === "1" ? tx("已连接") : tx("未连接")}</strong></div>
                </section>
              </main>
            </section>

            <section class="page-panel" aria-label={tx("扩展工具页面")}>
              <main class="page-content">
                <div class="page-intro"><div class="intro-icon"><ToolsIcon /></div><div><h1>{tx("音频扩展工具")}</h1></div></div>
                <section class="grid two-col extra-grid">
                  <article class="card tool-panel">
                    <SectionHeading title={tx("蓝牙音频 HAL")} />
                    <p class="field-help">{tx("选择系统使用的蓝牙音频实现。应用时会修改持久属性并重启音频 HAL。")}</p>
                    <SelectField title={tx("选择 Bluetooth HAL")} value={bluetoothHal()} options={localizeOptions(bluetoothHalOptions)} language={language()} onChange={(value) => setBluetoothHal(value)} />
                    <div class="inline-actions"><button class="primary-button" disabled={busy()} onClick={() => runExtra(["bluetooth-hal", bluetoothHal()], "正在切换 Bluetooth HAL…", "Bluetooth HAL 已切换")}>{tx("应用")}</button></div>
                  </article>

                  <article class="card tool-panel">
                    <SectionHeading title={tx("AudioFlinger 重采样器")} />
                    <p class="field-help">{tx("选择重采样质量预设，或自定义阻带衰减、滤波器长度与截止比例。")}</p>
                    <SelectField title={tx("选择重采样预设")} value={resamplerPreset()} options={localizeOptions(resamplerSelectOptions)} language={language()} onChange={(value) => setResamplerPreset(value)} />
                    <Show when={resamplerPreset() === "custom"}><div class="custom-resampler">
                      <label class="field-label">{tx("生效起始采样率")}</label><SelectField title={tx("选择生效起始采样率")} value={resamplerBypass()} options={localizeOptions(resamplerBypassOptions)} language={language()} onChange={(value) => setResamplerBypass(value)} />
                      <ToggleRow label={tx("Cheat 模式")} description={tx("关闭时使用标准 cutoff_percent。")} checked={resamplerCheat()} onChange={setResamplerCheat} />
                      <label class="field-label">{tx("Stop band（20–242 dB）")}</label><SelectField title={tx("选择 Stop band")} value={resamplerStopBand()} options={Array.from({ length: 223 }, (_, index) => { const value = String(index + 20); return [value, `${value} dB`] as const; })} language={language()} onChange={(value) => setResamplerStopBand(value)} />
                      <label class="field-label">{tx("Half filter length（8–640）")}</label><SelectField title={tx("选择 Half filter length")} value={resamplerHalfLength()} options={Array.from({ length: 80 }, (_, index) => { const value = String((index + 1) * 8); return [value, value] as const; })} language={language()} onChange={(value) => setResamplerHalfLength(value)} />
                      <label class="field-label">{resamplerCheat() ? "Cheat" : "Cutoff"} {tx("百分比")}</label><SelectField title={`${tx("选择") ?? "Select"} ${resamplerCheat() ? "Cheat" : "Cutoff"} ${tx("百分比")}`} value={resamplerPercent()} options={Array.from({ length: resamplerCheat() ? 200 : 100 }, (_, index) => { const value = String(index + 1); return [value, `${value}%`] as const; })} language={language()} onChange={(value) => setResamplerPercent(value)} />
                    </div></Show>
                    <div class="inline-actions"><button class="secondary-button" disabled={busy()} onClick={() => openConfirm({ title: "重置重采样设置", message: "将清除 AudioFlinger 重采样属性并恢复系统默认行为。", confirmLabel: "确认重置", action: () => void runExtra(["resampler", "reset"], "正在重置重采样…", "重采样已恢复系统默认") })}>{tx("重置")}</button><button class="primary-button" disabled={busy()} onClick={() => runExtra(resamplerPreset() === "custom" ? ["resampler", "custom", resamplerBypass(), resamplerCheat() ? "cheat" : "cutoff", resamplerStopBand(), resamplerHalfLength(), resamplerPercent()] : ["resampler", resamplerPreset()], "正在应用重采样配置…", "重采样配置已应用")}>{tx("应用")}</button></div>
                  </article>

                  <article class="card tool-panel">
                    <SectionHeading title={tx("USB 传输周期")} />
                    <p class="field-help">{tx("设置 USB 音频数据传输间隔。数值越低延迟越小，但可能降低播放稳定性。")}</p>
                    <div class="period-control" data-no-page-drag>
                      <div class="period-stepper">
                        <button type="button" aria-label={tx("减少 125 微秒")} onClick={() => stepUsbPeriod(-1)} disabled={busy() || Number(normalizeUsbPeriod()) <= 125}>−</button>
                        <label><input aria-label={tx("USB Transfer Period")} inputmode="numeric" type="number" min="125" max="50000" step="125" value={usbPeriod()} onInput={(event) => setUsbPeriod(event.currentTarget.value)} onBlur={() => setUsbPeriod(normalizeUsbPeriod())} /><span>μs</span></label>
                        <button type="button" aria-label={tx("增加 125 微秒")} onClick={() => stepUsbPeriod(1)} disabled={busy() || Number(normalizeUsbPeriod()) >= 50000}>+</button>
                      </div>
                      <input class="period-range" aria-label={tx("快速调整 USB Transfer Period")} type="range" min="125" max="50000" step="125" value={normalizeUsbPeriod()} onInput={(event) => setUsbPeriod(event.currentTarget.value)} />
                      <div class="period-scale"><span>125 μs</span><strong>{normalizeUsbPeriod()} μs</strong><span>50000 μs</span></div>
                    </div>
                    <div class="inline-actions"><button class="secondary-button" disabled={busy()} onClick={() => openConfirm({ title: "重置 USB Transfer Period", message: "将清除当前 USB 传输周期设置并恢复系统默认值。", confirmLabel: "确认重置", action: () => void runExtra(["usb-period", "reset"], "正在重置 USB period…", "USB period 已恢复系统默认") })}>{tx("重置")}</button><button class="primary-button" disabled={busy()} onClick={applyUsbPeriod}>{tx("应用")}</button></div>
                  </article>

                  <article class="card tool-panel">
                    <SectionHeading title={tx("音频诊断")} />
                    <p class="field-help">{tx("读取 AudioFlinger、蓝牙编解码、音频配置文件或 ALSA 设备状态。")}</p>
                    <SelectField title={tx("选择诊断类型")} value={diagnostic()} options={localizeOptions(diagnosticOptions)} language={language()} onChange={(value) => setDiagnostic(value)} />
                    <ToggleRow label={tx("完整输出")} description={tx("关闭时只保留关键字段。")} checked={diagnosticAll()} onChange={setDiagnosticAll} />
                    <div class="inline-actions"><button class="primary-button" disabled={busy()} onClick={runDiagnostic}>{tx("运行诊断")}</button></div>
                    <pre class="diagnostic-output" aria-live="polite">{diagnosticOutput() === "尚未运行诊断。" ? tx(diagnosticOutput()) : diagnosticOutput()}</pre>
                  </article>
                </section>
              </main>
            </section>

            <section class="page-panel" aria-label={tx("系统调优页面")}>
              <main class="page-content">
                <div class="page-intro danger-intro"><div class="intro-icon"><TuneIcon /></div><div><h1>{tx("系统 Jitter 优化")}</h1></div></div>
                <section class="card section-card danger-card">
                  <div class="notice danger-notice"><WarningIcon /><span><strong>{tx("风险提示：")}</strong>{tx("这些选项可能修改 SELinux、温控、系统服务、调度器和内核参数，可能降低系统安全性、稳定性或导致设备过热。请仅启用已了解影响的项目；SELinux 与温控选项应用时会再次确认。")}</span></div>
                  <div class="switch-grid jitter-grid"><For each={jitterFeatures}>{(feature) => <ToggleRow label={`${tx(feature[1])}${jitterDirty().includes(feature[0]) ? tx(" · 待应用") : ""}`} description={tx(feature[2])} checked={jitterValues()[feature[0]]} onChange={(value) => updateJitter(feature[0], value)} />}</For></div>
                  <Show when={jitterValues().io && jitterDirty().includes("io")}><div class="grid two-col io-options"><div><label class="field-label">I/O scheduler</label><SelectField title={tx("选择 I/O scheduler")} value={ioScheduler()} options={localizeOptions(ioSchedulerOptions)} language={language()} onChange={(value) => setIoScheduler(value)} /></div><div><label class="field-label">{tx("声音倾向")}</label><SelectField title={tx("选择声音倾向")} value={ioTone()} options={ioToneOptions} language={language()} onChange={(value) => setIoTone(value)} /></div></div></Show>
                  <Show when={jitterValues().wifi && jitterDirty().includes("wifi")}><div class="wifi-option"><ToggleRow label={tx("切换时不重启 Wi-Fi")} description={tx("使用 upstream 的 no-restart 模式。")} checked={wifiNoRestart()} onChange={setWifiNoRestart} /></div></Show>
                  <div class="inline-actions right tuning-actions"><button class="secondary-button" disabled={busy()} onClick={() => openConfirm({ title: "重置 Jitter 基础项", message: "将恢复 SELinux、温控、Doze、调频、相机、日志、I/O、虚拟内存和 Wi-Fi 的基础设置。", confirmLabel: "确认重置", action: () => void runExtra(["jitter", "disable", "all"], "正在重置全部基础 jitter 项…", "Jitter 基础项已重置") })}>{tx("重置")}</button><button class="primary-button" disabled={busy() || !jitterDirty().length} onClick={applyJitter}>{tx("应用")}</button></div>
                </section>

              </main>
            </section>

            <section class="page-panel" aria-label={tx("设置页面")}>
              <main class="page-content">
                <div class="page-intro"><div class="intro-icon"><SettingsIcon /></div><div><h1>{tx("设置")}</h1></div></div>
                <section class="card section-card">
                  <SectionHeading title={tx("语言")} />
                  <p class="field-help">{tx("选择 WebUI 显示语言。English 的界面翻译暂未实现。")}</p>
                  <div class="language-setting"><SelectField title={tx("选择界面语言")} value={language()} options={localizeOptions(languageOptions)} language={language()} onChange={(value) => setLanguage(value as Language)} /><button type="button" class="primary-button" onClick={applyLanguage}>{tx("应用")}</button></div>
                </section>

                <section class="card section-card about-card">
                  <SectionHeading title={tx("关于")} />
                  <div class="about-brand"><span class="brand-mark"><BrandIcon /></span><div><h3>USB SampleRate Changer WebUI</h3><p>{tx("Root audio policy 工具界面")}</p></div></div>
                  <div class="about-details"><div><span>{tx("模块版本")}</span><strong>{WEBUI_VERSION}</strong></div></div>
                  <p class="about-license">{tx("本项目基于 USB_SampleRate_Changer，遵循项目所附许可证。")}</p>
                </section>
              </main>
            </section>
          </div>
        </div>

        <nav
          class="page-navigation"
          classList={{ dragging: pageDragging() }}
          style={{ "--page-progress": pageProgress() }}
          aria-label={tx("主要页面")}
        >
          <span class="page-nav-indicator" aria-hidden="true" />
          <For each={pageItems}>{(item, index) => {
            const PageIcon = item.icon;
            return <button class="page-nav-item" classList={{ active: activePage() === item.id }} aria-current={activeIndex() === index() ? "page" : undefined} onClick={() => activatePage(item.id)}><PageIcon /><span>{tx(item.label)}</span></button>;
          }}</For>
        </nav>
      </div>

      <ToastHost toasts={toasts()} />

      <Show when={confirmRequest()} keyed>{(request) =>
        <div class="dialog-layer confirm-layer" data-no-page-drag role="presentation">
          <button class="dialog-backdrop" aria-label={tx("取消确认")} onClick={() => dismissConfirm()} />
          <section class="confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-message">
            <div class="confirm-mark" aria-hidden="true"><WarningIcon /></div>
            <div><h2 id="confirm-title">{request.title}</h2><p id="confirm-message">{request.message}</p></div>
            <div class="confirm-actions" classList={{ single: request.showCancel === false }}><Show when={request.showCancel !== false}><button type="button" class="secondary-button" onClick={() => dismissConfirm()}>{tx("取消")}</button></Show><button type="button" class="danger-button" onClick={acceptConfirm}>{request.confirmLabel}</button></div>
          </section>
        </div>
      }</Show>

      <Show when={policyHelpOpen()}>
        <div class="dialog-layer" data-no-page-drag role="presentation">
          <button class="dialog-backdrop" aria-label={tx("关闭模板说明")} onClick={() => closeOverlay("policy-help")} />
          <section class="info-dialog" role="dialog" aria-modal="true" aria-labelledby="policy-help-title">
            <header><div><h2 id="policy-help-title">{tx("策略模板说明")}</h2><p>{tx("模板决定生成 audio policy XML 时保留或绕过哪些输出路径；最终结果仍受 ROM、HAL 和设备能力影响。")}</p></div><button class="dialog-close" aria-label={tx("关闭模板说明")} onClick={() => closeOverlay("policy-help")}>×</button></header>
            <div class="policy-guide-list">
              <For each={policyOptions}>{([value, label, summary]) => <article classList={{ current: value === settings().policy }}><div><h3>{tx(label)}</h3><Show when={value === settings().policy}><span>{tx("当前选择")}</span></Show></div><p class="policy-summary">{tx(summary)}</p><p>{tx(policyDetails[value])}</p></article>}</For>
            </div>
          </section>
        </div>
      </Show>

      <Show when={logOpen()}>
        <div class="dialog-layer" role="presentation">
          <button class="dialog-backdrop" aria-label={tx("关闭执行日志")} onClick={() => closeOverlay("log")} />
          <section class="log-dialog" role="dialog" aria-modal="true" aria-labelledby="log-title">
            <header><div><h2 id="log-title">{tx("执行日志")}</h2></div><button class="icon-button" aria-label={tx("关闭执行日志")} onClick={() => closeOverlay("log")}>×</button></header>
            <pre>{log() || tx("暂无执行输出。运行状态查询、诊断或应用配置后会显示在这里。")}</pre>
          </section>
        </div>
      </Show>
    </>
  );
}

function ToastHost(props: { toasts: ToastItem[] }) {
  return <div id="toast-container" aria-live="polite"><For each={props.toasts}>{(toast) => <div class={`toast${toast.tone ? ` ${toast.tone}` : ""}`}>{toast.message}</div>}</For></div>;
}

function SectionHeading(props: { title: string }) {
  return <div class="section-heading"><h2>{props.title}</h2></div>;
}

function ToggleRow(props: { label: string; description: string; checked: boolean; disabled?: boolean; onChange: (value: boolean) => void }) {
  return <label class={`switch-row ${props.disabled ? "disabled" : ""}`}><span class="switch-copy"><strong>{props.label}</strong><small>{props.description}</small></span><input type="checkbox" checked={props.checked} disabled={props.disabled} onChange={(event) => props.onChange(event.currentTarget.checked)} /><span class="switch-track"><span /></span></label>;
}

render(() => <App />, document.getElementById("app")!);
