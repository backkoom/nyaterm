# 插件开发与排错

插件安装、授权、运行状态和日志都在 **插件面板** 中操作。

## 创建项目

在 NyaTerm 仓库根目录执行：

```sh
# 纯界面插件：查看选中终端的会话信息
pnpm plugin:create example.session temp/plugins/my-session --template ui
pnpm plugin:pack temp/plugins/my-session temp/plugins/my-session.nyap

# Rust 插件：界面调用后台，也提供原生命令
pnpm plugin:create example.hello temp/plugins/my-hello --template rust
node temp/plugins/my-hello/build.mjs
pnpm plugin:pack temp/plugins/my-hello temp/plugins/my-hello.nyap
```

创建器拒绝覆盖已有目录。项目包含 manifest、可用入口、SDK 和 README；
Rust 模板还包含编译脚本，编译后自动填入当前平台的可执行文件路径。
需要 Node 和 Rust 1.94 或更新版本。Rust SDK 使用本地路径依赖，移动项目时
应一起提供 SDK 或调整 Cargo.toml 中的路径。

将生成的 `.nyap` 安装到插件面板，检查权限并启用。UI 模板需先选中已连接终端；
Rust 模板可以打开界面点击运行，也可以直接执行插件卡片上的原生命令。

## 页面字体

宿主会将应用的界面字体栈通过 `NyaTerm.context.theme` 中的 `--font-sans`、
`--font-display` 同步到插件页面；修改设置时，已打开的页面也会收到更新，无需重载。
`--font-mono` 提供宿主的默认等宽字体栈。SDK 默认使用界面字体，表单控件继承字体；
插件自定义 CSS 应使用 `font-family: var(--font-sans, system-ui, sans-serif)`，
代码区域可使用 `var(--font-mono, monospace)`，避免写死 `system-ui`。

iframe 可以使用系统安装的字体，但不会继承主界面的 `@font-face`。
若使用未安装到系统的内置字体或其他 Web 字体，需要在插件包内提供字体文件并声明
`@font-face`。已有插件若写死字体，需要修改 CSS 并重新打包安装。

## 编写 Rust 后台

入口实现 `Plugin`，由 `Server::from_env()` 启动服务。SDK 负责握手、传输、
并发、作用域、超时和取消，无需手写 JSON-RPC：

```rust
#[async_trait]
impl Plugin for MyPlugin {
    async fn call(&self, context: Context, method: &str, input: Value) -> Result<Value> {
        match method {
            "ui/session" => {
                let session = context.host().session().await?;
                context.log("info", "Session inspected").await?;
                Ok(session)
            }
            _ => Err(RpcError::method_not_found(method)),
        }
    }
}
```

以上会话读取需在 manifest 中声明并获授 `session.read`；原生执行需 `native`。
UI 调用 `NyaTerm.backend("ui/session")`，contributions.commands 中的原生命令
则使用 manifest 声明的方法。不要向 stdout 打印日志，它专用于协议；使用
`Context::log` 或 stderr。不要记录口令、令牌、终端输入输出等敏感内容。

默认采用 JSONL。将 `backend.transport` 改为 `stdio-framed` 后，SDK 根据宿主
环境自动切换协议，支持二进制帧；界面流式订阅尚未提供。完整 API 和取消约定
见 [Rust SDK](../plugins/sdk/rust/nyaterm-plugin-sdk/README.md)。

## 固定远端 Probe 和共享 Monitor

Manifest v1 可选声明 `contributions.probes` 和 `contributions.monitors`，旧包不必增加字段。
以 GPU 为例：

```json
{
  "contributions": {
    "panels": [{ "id": "overview", "title": "GPU", "entry": "ui/index.html" }],
    "probes": [
      {
        "id": "gpu-overview",
        "title": "GPU probe",
        "entry": "assets/probes/gpu.sh",
        "timeoutMs": 15000
      }
    ],
    "monitors": [
      {
        "id": "gpu",
        "title": "GPU",
        "schema": "gpu.v1",
        "method": "monitor/collect",
        "panel": "overview"
      }
    ]
  },
  "permissions": ["native", "remote.probe"]
}
```

每类最多 16 项，ID 不可与其他贡献重复。脚本必须位于包内 `assets/probes/*.sh`，为非空 UTF-8、无 NUL、最多 64 KiB；
超时范围 1–30 秒。Monitor 必须引用已声明面板，声明 `monitor/*` 方法、原生后台和 Probe；当前只支持 `gpu.v1`。
自动授权绑定审核版本；启用对话框展示脚本全文，更新/切换版本移除 `remote.probe` 授权。
宿主不保证第三方脚本只读，不允许两个有效插件同时提供同一种 Monitor schema。

后台每次 `monitor/collect` 调用 `context.host().remote_probe("gpu-overview").await?`，解析结果并返回 GPU JSON。
对应 RPC `host/remote/probe` **只接受** `{ "probeId": "gpu-overview" }`，返回 `{ stdout, stderr, exitStatus }`。
不能提供命令、参数、stdin 或其他会话 ID；宿主从当前包解析脚本，在所属 SSH 连接的新 exec 通道执行 `sh -s`。
stdout/stderr 合计限 1 MiB，超时或取消关闭该通道，不向交互终端输入命令、不关闭 SSH 连接。
执行前后检查版本、授权、窗口、会话及锁屏；网络设备和禁止远程探测的会话不可执行。

UI 只订阅结果，不直接启动后台采集循环：

```js
await NyaTerm.ready;
const monitor = await NyaTerm.monitoring.subscribe("gpu", (snapshot) => {
  // revision, sessionId, overview, error, refreshing, paused
  render(snapshot);
});
await monitor.refresh();
await monitor.unsubscribe();
```

宿主以插件/版本/窗口/会话/Monitor 为键共享任务，订阅返回初始快照，再推送递增 revision。
Bridge 仅向对应 iframe 发送所属订阅；未开放通用原生事件透传或 UI 调用 `monitor/*`。
宿主消费者使用 typed `pluginApi.subscribeMonitor/refreshMonitor/unsubscribeMonitor`；
`subscribePluginMonitor` 负责先监听再订阅、初始快照竞态、revision 去重及释放。
每个 iframe 最多 16 个订阅；宿主最多 256 订阅和 128 共享任务。

SDK Context 提供 `language`、主题变量和 `monitorIntervalSeconds`。使用 `onContextChange` 同步语言/主题，
仅间隔变化时重新订阅。主窗口订阅 25 分钟续期；iframe 作用域按现有机制重建。
闲置任务不持有请求租约；后台每次只处理一次采集，不保存已经结束请求的 Context。

`gpu.v1` 结构与 `RemoteGpuOverview` 相同，宿主拒绝未知字段并限制设备/进程数量和文本长度。
资产回填只能来自当前会话的 GPU 数据，没有任意连接修改接口。
完整示例与构建命令见 [GPU 插件](../plugins/examples/gpu-monitor/README.md)。

## 查看状态和日志

原生后台按需启动并复用进程。卡片显示未启动、启动中、运行中、已停止或运行异常；
禁用插件和仅界面插件有独立标识。RPC 方法返回错误时，只记录调用失败，后台仍可
继续运行；进程退出、握手或协议失败则显示运行异常。

点击 **查看日志** 可刷新、复制和清空。日志包括时间、版本、级别、来源和消息。
日志弹窗每秒刷新，卡片状态每 2.5 秒刷新；关闭插件管理面板后停止轮询。
每个插件最多保留 100 条、64 KiB 消息与来源/版本文本，单条消息最多 2 KiB。
stderr 每次启动最多采集约 64 KiB，超过后继续排空而不保留。
日志只在内存中保留，退出应用即清除，不进入备份或云同步。
宿主会遮蔽常见凭据和作用域令牌，但插件作者仍需自行避免写入敏感内容。

**停止后台** 保留启用状态和权限。普通调用可启动新进程，进程内计数等状态会重置；
监控任务会同时暂停，定时器不会自行重新启动后台，手动刷新或重新启用可恢复。
插件有进行中的请求时会提示忙碌；先结束或关闭该操作，再停止后台。
禁用、更新、切换版本、锁屏和退出应用也会停止后台。旧进程的延迟消息不会覆盖
新启动进程的状态。更新插件必须提升版本号，同版本不同内容不能重新安装。

## 本地验证

```sh
node --test scripts/create-plugin.node-test.mjs
cargo test --manifest-path plugins/sdk/rust/nyaterm-plugin-sdk/Cargo.toml
cargo test --manifest-path src-tauri/crates/nyaterm-plugin-runtime/Cargo.toml --features sdk-fixture
pnpm exec vitest run src/components/plugins/PluginDiagnostics.test.tsx --maxWorkers=4
```

`native-counter` 示例已使用 SDK，并升到 1.0.1：执行两次可看到计数递增和日志；
停止后台后再执行，计数从 1 开始。新版安装包为 `temp/plugins/native-counter.nyap`。
模板编译和打包已在 Windows 验证；其他平台需构建对应二进制并填写 executables。
