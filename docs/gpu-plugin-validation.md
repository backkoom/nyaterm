# GPU 插件验证记录

验证日期：2026-10-03。宿主：Windows x86_64；目标远端：Linux/NVIDIA。
本记录验证代码、包和真实原生进程；没有连接用户的 SSH 会话，也没有真实 NVIDIA 主机结果。

## 交付

- 源码：`plugins/examples/gpu-monitor/`，插件 ID `nyaterm.gpu`，版本 `1.0.0`。
- 包：`temp/plugins/gpu-monitor.nyap`，Windows x86_64，484,490 字节，8 个 ZIP 条目。
- SHA-256：`1ce31438e052427f926068837d1643ca465ceb02f30d100055dab38535548264`。
- 用法与构建：[GPU 插件 README](../plugins/examples/gpu-monitor/README.md)。
- API：[插件开发说明](plugin-development.md)。

```sh
pnpm plugin:example:gpu
pnpm plugin:pack plugins/examples/gpu-monitor temp/plugins/gpu-monitor.nyap
node plugins/examples/gpu-monitor/verify-ui.mjs
```

构建输出包含相对路径 CSS、SDK、单个经典 IIFE 脚本和 release 后台。
打包器调用实际安装器校验路径、引用、配额及校验和后发布 `.nyap`；另行核对全部 checksums。
当前包只声明 Windows x86_64；构建脚本支持在 Linux/macOS、x86_64/aarch64 宿主重建相应产物。
不能据此宣称跨平台桌面验证完成。

## 自动验证

| 检查                                        | 结果与范围                                                                                                                       |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| Runtime `cargo test --features sdk-fixture` | 28 项通过；另有 1 项最终包测试需显式启用（本次已执行通过）                                                                       |
| 最终包 Sidecar 测试                         | 1 项通过；实际安装、授权、启动 release 可执行文件，两次 reverse Probe RPC 往返及 GPU 解析                                        |
| 共享 GPU crate                              | 5 项解析测试通过；涵盖多设备、缺失可选值、进程索引和带逗号名称                                                                   |
| Rust SDK                                    | 2 项测试通过；JSONL/framed 的进程互操作另由 Runtime 覆盖                                                                         |
| Runtime/SDK/GPU Clippy                      | `--all-targets -- -D warnings` 通过；Runtime 启用 `sdk-fixture`                                                                  |
| Tauri                                       | `cargo check --all-targets`、`cargo clippy --all-targets` 通过；宿主 Clippy 有项目级警告，未宣称零警告                           |
| 前端 TypeScript                             | 宿主 `tsc --noEmit` 与插件独立 TypeScript 检查通过                                                                               |
| 宿主/插件构建                               | `pnpm build` 和 `pnpm plugin:example:gpu` 通过；宿主仍有既有大 chunk/Browserslist 提示                                           |
| 前端 lint                                   | `pnpm lint` 通过；`CommandSuggestions.tsx` 有既有依赖数组警告                                                                    |
| 相关 Vitest                                 | 初始 7 文件 16 项通过；补充 Bridge 和续期失败检查后，相关 5 文件 16 项通过                                                       |
| 全量 Vitest 首轮                            | 155 文件中 154 通过，971 项通过；AI 设置有 1 项 5 秒超时。该文件独立重跑 9 项全部通过                                            |
| 全量 Vitest 最终重跑                        | `vitest run --maxWorkers=4`：155 文件、974 项全部通过                                                                            |
| 宿主 Rust 插件测试                          | 编译成功；测试可执行文件启动失败，Windows 返回 `0xc0000139 / STATUS_ENTRYPOINT_NOT_FOUND`，未执行断言                            |
| 构建产物 UI 冒烟                            | jsdom 执行实际 SDK 和 IIFE：设备/驱动/CUDA/温度/功率/风扇/显存/进程、排序/搜索、刷新、四语言、主题、保留数据的错误和暂停状态通过 |

Runtime 新增测试覆盖旧包兼容与新贡献校验、固定脚本全文审核、非法路径/超时/输出配额、更新和回滚移除授权、
GPU Monitor 冲突、共享需求、无重叠/补发、最后需求释放、取消和迟到结果、停止/手动恢复、不可用停止及三次失败清空。
前端测试覆盖初始快照竞态、revision 去重、订阅/iframe 归属、配额、取消、作用域续期、会话/激活切换、锁屏释放需求、
插件错误不切源、停用恢复内置、当前会话资产回填及 NPU 入口保持原行为。

最终包测试执行真实原生程序，使用安装包内脚本和 `execute_declared`。
执行器模拟远端输出，未建立网络 SSH 连接。私有 exec 通道代码经过 Tauri check/Clippy；
`SessionCommand::Write` 不在 Probe 执行路径中。尚未实际验证 russh 网络取消或终端无输入干扰。
jsdom 只验证构建产物逻辑和 DOM，不等同于 WebView2 的样式、opaque-origin 自定义协议或原生桌面验收。

## 用户 SSH 会话上的桌面验收（未执行）

使用包含本次接口实现的宿主，由用户自行建立 Linux/NVIDIA SSH 会话：

1. 安装并审核固定脚本，授权 `native`/`remote.probe` 后启用；确认普通终端输入不被采集打断。
2. 同时打开原 GPU 工作区和独立插件面板，设置 3 秒间隔，确认持续更新且共享采集。
3. 顶部状态选择 GPU，关闭两处面板，确认顶部状态仍更新；关闭全部 GPU 需求后确认停止。
4. 测试手动刷新、搜索/排序、四种语言和明暗主题；核对实时指标与服务器 `nvidia-smi`。
5. 在任务空闲时点击“停止后台”，确认定时器不重启后台；刷新后恢复。
6. 切换会话、断开、锁屏、关闭窗口和撤销授权，确认停止请求、旧数据不回填新会话。
7. 超过 25 分钟确认主窗口续期不重载工作区数据；iframe 重建后取得当前快照。
8. 模拟采集错误，确认保留旧数据并标记错误，三次后清空；不静默回退。
9. 停用后确认恢复内置采集；更新/回滚要求重新审核；第二个 GPU Monitor 冲突被拒绝。

真实 NVIDIA 数据、持续网络采集、网络取消、原生安装 UI 和上述桌面行为均未验收。
本次没有修改用户凭据、建立用户会话或安装到用户的真实插件目录。
