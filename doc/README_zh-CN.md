# USB SampleRate Changer WebUI（中文参考）

这是项目根目录 [`README.md`](../README.md) 的中文参考译文。英文 README 与实际构建脚本保持同步，若两者存在差异，请以英文 README、源代码和设备上的实际行为为准。

本项目将 [SivonZn/USB_SampleRate_Changer](https://github.com/SivonZn/USB_SampleRate_Changer)（基于[原始项目](https://github.com/yzyhk904/USB_SampleRate_Changer)的 Fork）打包为 KernelSU/APatch WebUI 模块。模块没有开机服务；打开 WebUI 后，才会按需调用模块内置的 `usbsrctl` 控制器。

Fork 源码以 [`USB_SampleRate_Changer`](../USB_SampleRate_Changer) Git 子模块的形式附加。父仓库通过 Gitlink 固定一个可复现的子模块提交；[`patches/series`](../patches/series) 中列出的补丁只会在构建时应用到临时副本。子模块工作区保持独立，可用于排查问题、开发通用修复，或向上游项目提交改进。

## 仓库结构

| 路径 | 用途 |
| --- | --- |
| `USB_SampleRate_Changer/` | Fork 子模块及可复现的源代码提交 |
| `patches/` | 等待上游验证的下游临时修改 |
| `module/` | KernelSU/APatch 模块元数据和安装覆盖文件 |
| `backend/` | Rust 参数校验器和控制器 |
| `webui/` | SolidJS WebUI 源代码 |
| `build.sh` | 获取、打补丁、编译和打包流程 |
| `doc/` | 项目文档与中文参考译文 |

构建出的模块会将上游脚本、模板和 Extras 全部放在模块根目录的 `core/` 中；根目录仅保留模块元数据、安装脚本、控制器和 WebUI 文件。Magisk/KernelSU 要求的 `customize.sh` 与 `uninstall.sh` 仍位于模块根目录。

## 参数对应关系

Rust 控制器暴露了 `USB_SampleRate_Changer.sh` 所解析的全部行为参数：

| WebUI 控件 | 上游参数或行为 |
| --- | --- |
| 策略模板选择器 | `--auto`、`--offload`、`--offload-hifi-playback`、`--offload-direct`、`--offload-safer`、`--bypass-offload`、`--bypass-offload-safer`、`--legacy`、`--safe`、`--safest`、`--safest-auto`、`--usb-only` |
| 采样率选择器 | 文档列出的 44.1–768 kHz 预设，以及上游支持范围内的 44100–768000 Hz 自定义整数 |
| 位深选择器 | `16`、`24`、`32`、`float` |
| DRC 开关 | `--drc` |
| 强制 USBv2 开关 | `--force-usbv2` |
| 强制 Bluetooth QTI 开关 | `--force-bluetooth-qti` |
| Bluetooth A2DP 路由检查 | 仅当执行前检测到 A2DP 已连接时，在音频修改后自动运行 |
| Amazon Music 模板开关 | `--amzm` |
| 测试模板开关和选择器 | `--test --test-template NAME` |
| 重置按钮 | `--reset` |

## Extras 功能

上游 `extras/` 目录中的脚本也通过固定的 Rust 子命令提供给 WebUI。WebUI 不接受任意脚本名称，也不允许输入自由格式的 Shell 命令。

| WebUI 区域 | 上游脚本及支持的控制项 |
| --- | --- |
| Bluetooth HAL | `change-bluetooth-hal.sh`：状态查询，以及 `aosp`、`legacy`、`offload`、`sysbta` 切换 |
| AudioFlinger 重采样器 | `change-resampling-quality.sh`：状态/重置、所有内置预设，或经过校验的自定义阻带衰减（20–242 dB）、半滤波器长度（8–640，步进 8）、cutoff/cheat 百分比和 44.1/48/96 kHz 生效阈值 |
| USB 传输周期 | `change-usb-period.sh`：状态/重置，以及完整的 125–50000 μs 范围（步进 125 μs） |
| Jitter reducer | `jitter-reducer.sh`：SELinux、温控、Doze、调频 governor、相机、logd、I/O、虚拟内存、Wi-Fi、电池和音效；支持 I/O scheduler/tone 以及 Wi-Fi no-restart |
| 诊断 | 过滤/完整音频 dump、Bluetooth dump、音频配置探测和 ALSA 硬件参数 |

启用 SELinux 或温控 reducer 时，WebUI 会要求二次确认。这些功能可能削弱设备安全性或温控保护。Jitter 开关表示待执行动作，而不是持久化状态：上游的大多数 Jitter 修改不会跨重启保留，Wi-Fi 相关行为可能会保留。

`--help` 只用于显示帮助信息，不会被保存。策略参数互斥。由于 `--amzm` 和 `--test` 都会覆盖所选策略模板，WebUI 也会强制二者互斥；按照参数处理顺序，后者只会在启用测试模式时生效。

上游默认的 `templates/test_template.xml` 并不存在，因此测试模式必须从内置 `templates/` 目录枚举出的 XML 文件中选择模板。控制器会拒绝任意路径。

## 执行路径

```text
SolidJS WebUI
  -> window.ksu.exec(usbsrctl apply ... | usbsrctl extra ...)
  -> Rust 校验固定的参数模式
  -> /data/adb/usb_samplerate_changer_webui/generated/*.sh
  -> 必要时切换到 audioserver 的 mount namespace
  -> 执行模块 `core/` 中选定的上游脚本
```

当控制器不在 audioserver namespace 中启动时，会通过 `su --mount-master` 重试生成的脚本。音频策略应用/重置脚本还会再次比较 namespace，避免 namespace 切换静默失败后错误报告成功。Extras 脚本使用相同的控制器路由和操作锁。

重启 `audioserver` 后，已经连接的 Bluetooth 耳机可能只注册为 SCO，即使 Bluetooth 栈仍然有活动的 A2DP codec。控制器会记录应用/重置前的 A2DP 状态，并在服务重载后同时检查已连接设备列表和 `STREAM_MUSIC` 路由。应用前 WebUI 会在 A2DP 已连接时给出警告；如果重载后的路由检查失败，控制器返回退出码 72，WebUI 会询问是否打开 Bluetooth 设置，让用户手动断开并重新连接设备，而不会自动跳转或错误报告成功。

最新配置、生成脚本、状态和输出会以仅 root 可读的权限保存到 `/data/adb/usb_samplerate_changer_webui/`。

## 构建

构建需要 Node.js、Rust、`aarch64-linux-android` Rust target、Android NDK 和 `zip`。克隆仓库时建议同时初始化子模块：

```sh
git clone --recurse-submodules https://github.com/SivonZn/USB_SampleRate_Changer_WebUI.git
# 已有仓库：
git submodule update --init --recursive
```

构建流程会将当前子模块工作区复制到临时目录，校验并应用所有列出的补丁，编译两个应用，最后在上级 Magisk 工作区的 `output/` 目录生成模块：

```sh
./build.sh
```

子模块中的未提交修改会被有意纳入本地构建，因此可以立即测试上游脚本的本地改动：

```sh
cd USB_SampleRate_Changer
git switch -c fix/example
# 编辑或提交上游修复，然后返回 WebUI 仓库。
cd ..
./build.sh
```

在本仓库记录新的子模块 revision 前，应先将相应修改提交并推送到 Fork，再使用 `git add USB_SampleRate_Changer` 暂存更新后的 Gitlink。

当前成品名称为 `USB_SampleRate_Changer_WebUI-0.4.0.zip`。同版本重新构建时会覆盖同名文件。

开发检查：

```sh
cargo test --manifest-path backend/Cargo.toml
cd webui && npm run build
```

## 安全与兼容性提示

- 本模块会修改音频策略、音频服务属性以及部分系统调优参数。请确认设备、ROM 和 HAL 支持所选模式。
- 应用策略或重采样配置可能短暂中断音频；Bluetooth A2DP 已连接时尤其需要注意路由恢复提示。
- SELinux、温控、Doze、调频和系统服务相关操作可能降低安全性、稳定性或续航，并可能导致设备过热。请只启用自己理解其影响的选项。
- 高采样率是否真正可用还取决于 USB DAC、内核、USB Audio HAL 和厂商音频策略；选择器中的范围不代表每台设备都支持全部采样率。
- 本项目不对尝试使用本模块可能造成的设备损坏负责，是否使用由用户自行决定。
