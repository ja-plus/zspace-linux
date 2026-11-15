# ZSpace NAS Agent Toolkit

独立的极空间 NAS Rust CLI 与 Agent Skill 包，不依赖 WorkBuddy Connector 运行时。

## 客户端正式版本 0.3.11

本版新增小体积在线安装模式；登录仍由 Agent 在当前会话托管 CLI 进程，不要求用户另开终端或回到终端粘贴授权码。
平台支持、在线缓存行为与验收边界见 [发布说明](reference/release-0.3.11.md)。
服务端不在本次交付范围。六个平台均提供预编译文件；其中未完成原生验收的平台属于预览支持，不能据此推断其全部业务能力已通过验收。

## 直接使用

macOS/Linux 由包装脚本自动选择当前架构的二进制：

```bash
scripts/zspace-cli --profile <profile> login --no-open --auth-code-stdin
scripts/zspace-cli --profile <profile> doctor
scripts/zspace-cli --profile <profile> file space --params '{}'
```

由 Agent 在当前会话执行登录命令，并从 stdout 中找到 `event` 等于 `oauth_authorization` 的那行 JSON，读取其顶层 `url` 字段作为授权链接。用户点击链接完成网页授权后，将一次性授权码回复给 Agent；Agent 仅把该码写入当前进程 stdin，不要求用户另开终端或自行操作 CLI。该一次性码会经过当前对话，但不得复述、持久化或写入日志。

Windows PowerShell/CMD 使用 `scripts/zspace-cli.ps1` 或 `scripts/zspace-cli.cmd`；Git Bash 可直接使用 `scripts/zspace-cli`。thin 包会按平台下载并验证 CLI，offline 包才在 `bin/` 内携带六个预编译产物：

- macOS: x86_64、ARM64
- Linux: x86_64、ARM64（静态链接 musl）
- Windows: x86_64、ARM64

使用 `bin/SHA256SUMS` 校验发布文件。OAuth 凭证默认保存到系统安全存储；仅当用户显式选择文件后端，
或系统安全存储不可用时，才写入权限收紧的 profile 文件。

thin 包不含 `bin/`：启动器首次运行时根据包内固定版本清单下载当前平台二进制，并在每次执行前核对字节数和 SHA-256。缓存按版本隔离；升级 Skill 不会跟随可变 `latest`。离线或受限网络环境使用 offline 包。

Windows PowerShell 5.1 启动器会将控制台输入输出设为 UTF-8，并通过显式 Windows 命令行引号编码传透 `--params` JSON；不要在 JSON 内额外加反斜杠。
它仅在显式使用 `--auth-code-stdin` 时把当前标准输入转发给 CLI；其他命令不会等待 stdin EOF。

## 开发

```bash
cargo fetch --locked
cargo test --locked
scripts/build-release.sh
scripts/zspace-cli --help
```

构建可供 Agent 上传安装的 Skill 压缩包：

```bash
./build.sh --thin
./build.sh --offline
```

产物分别为 `dist/zspace-nas-agent-toolkit-v<version>-thin.zip` 与 `-offline.zip`。两者均保留单一顶层目录；thin 包不含二进制且硬性限制为2.5MiB，offline 包包含六个平台。需要先重新编译时增加 `--rebuild`。

完整 Agent 使用方式见 `SKILL.md`；跨平台构建见 `scripts/build-release.sh`。
发布编译使用锁定的离线 Cargo 依赖；首次构建先执行 `cargo fetch --locked`。
六平台构建另需 Rust targets、cargo-zigbuild、cargo-xwin、Zig/LLVM 和 MSVC 工具缓存。

## 发布与覆盖范围

完整包包含六平台二进制、运行入口、参考文档、许可证、第三方声明和哈希清单。
使用 `release-manifest.json` 核对当前源码及二进制快照。
业务覆盖范围见 [覆盖矩阵](reference/coverage-functions-2026-10-15.md)，实际命令以 `catalog` 为准。
本版不宣称已覆盖全部设备、硬件或跨账号场景。
