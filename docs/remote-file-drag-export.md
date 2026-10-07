# 文件／目录拖拽下载

现有普通下载链路为 `transfer_paths` 选择本地目标，
`enqueue_sftp_download_job_for_target` 提交后台作业，随后调用
`RemoteFileService::download_remote_path_with_progress_and_path_options`。
它已经处理递归目录、进度、暂停／取消、重名策略、原始远程路径，以及
`DownloadTemporary` 的文件暂存／提交。

原来的拖出只支持 Windows 远程普通文件：GPUI 手势冻结选区，离开来源窗口后
解析虚拟文件 payload；Explorer 在放下后通过 `FILECONTENTS` 请求内容；
`SftpReadSource` 将有界同步读取转到既有 SFTP runtime。它不知道 Explorer 的
目标路径，所以不能把 Windows 拖出改成调用普通下载函数。

## 本轮实现

| 平台 | 文件／目录流程 | 本地目标及冲突处理 |
| --- | --- | --- |
| Windows | OLE STA 枚举目录描述符，文件内容仍在 Drop 后按需读取 | Explorer 选择目标、解决冲突并落盘，应用不虚构目标路径 |
| macOS | `NSFilePromiseProvider` 广告顶层文件／目录，Finder 提供最终目标 URL 后在 `NSOperationQueue` 调用既有递归下载 | 使用 Finder 实际交付的完整文件路径；已占用目标返回错误，不擅自覆盖或改名 |
| Linux Wayland | 首次拖出准备本地下载，完成后提示再次拖出；第二次通过 `text/uri-list` 导出真实路径 | 文件管理器复制已完成的本地文件／目录，原生拖出仅提供 Copy |
| Linux X11 | 同样先准备，再通过独立 X11 连接执行 Xdnd URI 拷贝 | 实现 Enter／Position／Status／Drop／Finished、选择数据请求、Esc 与窗口关闭取消 |

列表和树视图都支持普通文件、目录及多选。树中同时选择目录及其子项时，按
原始远程路径去除重复子项，只导出该目录一次。显式选择链接或特殊文件会
拒绝整个选区；递归目录中不跟随链接／不下载设备，沿用普通目录下载策略。

## 分层和生命周期

- `nyaterm-transport::drag_export::tree` 在 worker 上枚举元数据，保留 raw path
  token，重新校验类型，保留空目录，限制深度和总项目数。普通文件流不先
  完整下载。Windows 的父目录描述符排在子项之前，文件保留稳定 `lindex`。
- GPUI 的 deferred tree 只解析一次，失败也被冻结；取消不会触发枚举。Windows
  广告前校验所有相对路径，拒绝 traversal、ADS、设备名、缺失父目录、重复
  路径和超长 UTF-16 名。目录只有 descriptor，没有 `FILECONTENTS` stream。
- `TransferSelection` 冻结来源会话、服务 Weak、选区和传输选项；切换标签或
  改选区不能重定向正在进行的手势。普通控件渲染和 payload resolver 不做
  文件系统／网络 I/O。列表和树共用视图中的手势适配器；重命名输入不启动拖出。
- Finder 和 Linux 暂存复用现有递归下载、暂存文件、暂停／取消和进度；后台
  typed events 更新传输队列。Finder 已知目标显示为普通下载；Windows 及
  Linux 暂存不提供假的最终本地目标或重试到暂存路径。
- 来源服务 Weak 消失会取消正在进行的 promised/staging 下载。Finder delegate
  通过 provider 的 `userInfo` 保持有效，释放后取消 provider，不持有 GPUI Entity。
  原生 completion 对成功、失败和 panic 都只调用一次。
- Linux 的准备状态由 transfer feature 中一个状态机拥有。准备完毕前不广告
  URI；失败会清理整批暂存。完成结果只供下一次拖出消费，避免后续手势复用旧
  下载；会话及来源服务身份是准备键的一部分，结果在一小时后失效。
  状态保留来源 Weak，防止断线重连后新服务复用旧准备键的内存地址。
- 成功暂存源必须允许文件管理器在应用关闭后继续复制，因此不在 drag end
  立即删除。后台下次准备时清理七天前的本应用暂存目录；临时目录使用私有权限。
  Linux 第一次手势启动准备后，可在传输队列取消；操作系统复制发生在第二次手势。

```text
Windows：冻结选区 → OLE worker 清单 → Drop → 有界 SFTP 流 → Explorer 落盘
macOS：冻结选区 → Finder promise → 最终 URL → 后台递归下载 → completion
Linux：冻结选区 → 后台暂存 → 准备完成提示 → 再次拖出 → 本地 URI Copy
```

## 范围和限制

远程拖出当前要求 SFTP；SCP 后备连接仍可使用普通“下载”。文件服务器可能在
枚举或读取期间修改内容，这不是远端内容快照。Windows 队列完成表示已经提供
内容，不代表 Explorer 成功保存了最终文件；Linux 的准备完成也不代表文件
管理器已经复制完成。文件暂存沿用既有提交逻辑，目录下载不是整棵树的原子事务。

Linux 没有广告 XDS／直接保存协议，也不声称每个文件管理器都支持 Xdnd；不支持
原生 URI 拖放的目标仍需使用普通下载。交互验证应覆盖 Explorer、Finder、
Wayland 和 X11 文件管理器的多选、空目录、重名、取消和大文件哈希。

未改变配置、credentials、redb schema、backup 或 cloud sync 数据契约。
未添加凭据或内容日志，未编辑 `temp/vendor`。

## Fork 和验证

补丁按关注点提交并推送到 `nyakang/zed` 的 `nyaterm` 分支：GPUI 契约、Windows
目录、Finder promise、Linux Xdnd、原生 ABI 修正及回归测试分别保留提交。
`NYATERM.md` 记录原因、验证和手动验证限制。

- Zed 固定 revision：`1322967fc37535ca307c539181fc16ad3d5334e9`。
- gpui-kit 固定 revision：`c80af68d0e3953a746c38d05577f547e897c4f16`；所有 Zed
  依赖一起更新，`check-gpui-pin.ts` 通过，未更新无关 registry 版本。
- Windows GPUI virtual suite：6 项通过；Windows native export suite：11 项通过。
- 新增 transport 测试覆盖原始字节路径、空目录、枚举不读取内容、来源类型变化、
  提前取消、树选区去重以及失败暂存清理。
- 新增 desktop 测试覆盖平台 payload 选择、准备状态消费和树视图实际拖拽。
- 最终 revision 的 macOS、Linux、Windows fork 原生编译通过，macOS 原生
  promise dispatch 回归测试通过：[三平台 CI](https://github.com/nyakang/zed/actions/runs/37606931560)。
  该测试验证 Objective-C 回调 ABI、delegate 生命周期、精确目标 URL，以及
  成功／失败时 completion 恰好调用一次。
- 主仓库在 Windows 上验证最终固定依赖：
  `cargo check --workspace --locked`、
  `cargo test --workspace --locked -- --test-threads=1`、
  `cargo clippy --workspace --all-targets --locked`、
  `cargo fmt --all -- --check` 和 `git diff --check` 全部通过，Clippy 无警告。
  工作区共 3476 项测试通过、15 项忽略，其中 desktop 1810 项通过、
  transport 416 项通过。
- 并行回归中 `windows_local_session_close_releases_conpty_reader` 两次达到
  五秒超时；独立运行及最终串行工作区回归都通过。仍需保留这一并行运行限制，
  不能从串行通过推断并行超时已被解决。

本轮没有把原生编译和 mock 协议测试表述为 Finder／文件管理器的手动端到端验证。
