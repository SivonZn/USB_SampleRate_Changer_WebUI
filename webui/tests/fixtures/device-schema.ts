import { parseControllerSchema } from "../../src/platform/controller-schema";
import { jitterFeatures } from "../../src/domain/options";

export function deviceSchema(limited: boolean) {
  const controls = !limited;
  return parseControllerSchema(JSON.stringify({
    schema_version: 1, api_version: 1,
    capabilities: { device_capabilities: true },
    device: { audio_hal: limited ? "aidl" : "legacy-xml", mode: limited ? "limited" : "full", reason: "" },
    limits: {
      sample_rate: { min: 44100, max: 768000 }, usb_period: { min: 125, max: 50000, step: 125 },
      resampler: { stop_band: { min: 20, max: 242 }, half_length: { min: 8, max: 640, step: 8 },
        cutoff_percent: { min: 1, max: 100 }, cheat_percent: { min: 1, max: 200 } }
    },
    policy: { available: controls, default: "auto", options: controls ? ["auto"] : [] },
    sample_rates: controls ? ["44100"] : [], bit_depths: controls ? ["32"] : [], switches: [],
    extras: {
      bluetooth_hal: { available: controls, operations: { set: controls }, options: ["offload"] },
      usb_period: { available: controls, operations: { set: controls, reset: controls } },
      resampler: { available: true, operations: { set_preset: true, set_custom: true, reset: true },
        presets: ["179-408-99"], custom: { value: "custom", bypass_options: ["none"] } },
      diagnostics: { available: true, operations: { run: true }, types: ["audio", "bluetooth", "config", "alsa"] },
      jitter: { available: true, operations: { set: true, reset: true },
        features: jitterFeatures.map(([value]) => ({ value, label_key: `jitter.${value}.label` })),
        reset_features: jitterFeatures.map(([value]) => value) }
    }
  }))!;
}
