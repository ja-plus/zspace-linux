# 极空间原生 Linux 版 · 挂载线（mountV2）

> 范围：只做「挂载云盘/文件」这条线；不改 `native/player/`、`native/libzrpc/`，不执行 git 操作。
> 结论先行：**当前客户端（V2.46.2026101501）实际走的是 `mountV2`（UZMountMonitor + 内嵌 rclone，
> WebDAV 后端），不是旧的 `mounts3.dll` + S3 路径。** 本目录按真实路径实现。

---

## 1. 与最初判断的差异（重要）

最初的口径是「写 rclone.conf（`type=s3 / provider=Minio`）→ `mounts3.dll.MountBucketEX`」。
这段代码确实还在 `background.js` 里，但已经被 `mountV2` 取代：

- `global.supportMountV2=!0`（background.js 固定置真）；
- 版本闸门 `webClientVersion.split(".").pop() >= "2025070101"`，
  当前版本 `2026101501` 满足；
- `mountS3IfNeeded` / `go()` 走 `po()`（mountV2），旧 `mn.mountS3` 只在 `mountV2` 不可用时兜底；
- mountV2 的配置实体是 **WebDAV**，不是 S3：

```ini
[uzmount]
type = webdav
url = http://127.0.0.1:<13600 代理端口>
vendor = other
user = <NAS 用户名>
pass = <rclone obscure 后的值>
```

证据与原始片段见 [research/evidence.md](research/evidence.md)。

---

## 2. 官方挂载的真实行为（已验证）

### 2.1 组件

| 组件 | 实测 | 作用 |
|---|---|---|
| `bin/ZSpaceMountMonitor.exe` | Go 1.20.14，`1.1.1-20251111174657`，CLI 名 `UZMountMonitor` | 编排：写 conf、拉 rclone、查状态、卸载 |
| `bin/ZSpaceMount.exe` | **rclone v1.70.2-20250702150924-windows-amd64**（go1.23.8, tags cmount） | 真正的挂载进程 |
| `bin/filter.txt` | 50 条排除规则（`.DS_Store`/`@eaDir`/`Thumbs.db`/`*.tmp`…） | `--use-filter` 时传给 rclone |
| 9001 代理 | `encodeService({local_port:13600, remote_port:9001, type:1, nas_id, request_purpose:8})` | 把本地端口打到 NAS 9001 |

Monitor 的完整子命令面（Wine 实测 `--help`）：

```
check / config / disk / disk-free-space / drivers / fuse / network / port
process / rc / sys / version / check-port-free / only-check-port-free
completion / random-free-port
```

### 2.2 目录与文件

`home-dir = <userData>/<username>[_<nasId>]/mount`，其中：

| 文件 | 内容 |
|---|---|
| `mount.conf` | 上面那份 webdav 配置 |
| `mount.port` | rclone rc 端口（JS 读它拼 `http://127.0.0.1:<port>/mount/status`） |
| `mount.lock` | 监控器单实例标记 |
| `mount_monitor.log` | 监控器日志（含 `[MOUNT] start mount, PID: %d, command: %s`） |
| `filter.txt` | 由 JS 从 `bin/filter.txt` 复制 |
| `usage.txt` | 总容量 / 可用空间（供 fork 的 `--vfs-disk-space-info-from` 使用） |

### 2.3 rclone 启动参数

从 `ZSpaceMountMonitor.exe` 反汇编出的字符串（`--help`/命令行常量）与 JS 传参合并：

```
<program> --config=<home>/mount.conf mount <section>: <mountpoint>
  --cache-dir=<cache> --vfs-cache-mode=full --vfs-cache-max-size=<size>
  --vfs-cache-max-age=10m --attr-timeout=5m --file-perms=0777
  --dir-cache-time=1m --vfs-write-back=1s --stats=5s
  --volname=<volume-label> --rc --rc-addr=127.0.0.1:<port> --rc-no-auth
  [--filter-from=<home>/filter.txt]
  # 否则默认加：
  --filter=- .*/  --filter=- .*
```

- 默认隐藏所有点开头文件/目录；`--show-hidden` 时两条内置过滤都不加。
- `--network-mode` 仅 Windows 有实际意义；`--rc-o=-vv` 透传为 rclone 的 `-vv`。
- 缓存上限：官方日志实测「disk free space 4.808Gi < 10Gi → set to 2.404Gi」，
  即空间不足时取可用空间一半。
- 挂载点：Windows 用盘符（`--volume=Z:`）；macOS 用 `<cachePath>/极空间-个人`
  （卸载处 `umount -f <cachePath>/极空间-个人` 可证）。**Linux 没有官方分支**，
  本实现按 mac 规则推导为 `<cache-dir>/<volume-label>`。

### 2.4 卸载

JS `_o()` → `fo()`：

1. `process --list --name=ZSpaceMount*`（同时匹配监控器与挂载进程）；
2. `rc --rc-addr=http://127.0.0.1:<port>/shutdown`；
3. 等 2s，`process --kill --pid=<monitor>`、`process --kill --pid=<mount>`。

### 2.5 fork 与上游 rclone 的差异

官方 `ZSpaceMount.exe` 是 rclone fork，额外提供：

- rc 端点 `/mount/status`、`/mount/start`、`/mount/restart`、`/shutdown`；
- 参数 `--vfs-disk-space-info-from=<usage.txt>`。

上游 rclone 没有这些。本实现由监控器补上 `/mount/status`（TCP 探活）与
`/shutdown`（转 `core/quit`），并**丢弃** `--vfs-disk-space-info-from`。

---

## 3. 我们的实现

`native/mount/`（Rust，无第三方依赖）：

```
src/main.rs   zspace-mount-monitor
```

实现与官方 `UZMountMonitor` 相同的命令行契约：

| 子命令 | 行为 |
|---|---|
| 根命令（无子命令） | 读 `mount.conf` → 起 rclone mount → 写 `mount.port`/`mount.lock` → 监控子进程并最多重试 3 次 |
| `config --add` | 写 `[uzmount] type=webdav ...`，`--obscure` 时调用 `<program> obscure` |
| `check` | 返回 `can_read/can_write/is_volume_valid/is_support_sparse/is_cloud/fuse_status/...` |
| `fuse --status` | 返回 `[{name:"fuse3",is_installed:true}]`（/dev/fuse + fusermount3 存在时） |
| `process --list/--kill` | 扫描 `/proc/*/comm`，按前缀匹配（自动去掉 `*`） |
| `rc --rc-addr` | `/mount/status` 探活、`/shutdown` 转 `core/quit`，其它路径 POST `{}` 转发 |
| `disk --size` | 递归统计缓存目录字节数 |
| `port/random-free-port/check-port-free` | 端口探测 |
| `network/sys/drivers` | Linux 无声明的部分返回成功空值（`network` 官方仅 mac 用） |

编译：

```bash
cd native/mount && cargo build --release
# 产物 native/mount/target/release/zspace-mount-monitor
```

### 3.1 主线接入（需要 patch_app.py / 打包侧改动，本线未改）

1. 把 `zspace-mount-monitor` 装成 `app/.../bin/ZSpaceMountMonitor`（0755，**无 `.exe`**）
   —— JS 的 `process --list --name=ZSpaceMount*` 依赖这个进程名。
2. 把上游 rclone 装成 `app/.../bin/ZSpaceMount`（0755），或把 JS 的 `wn` 指向 `rclone`；
   监控器通过 `--program` 找到它，`config --add` 也用它做 obscure。
3. deb `Depends` 增加 `fuse3`（提供 `fusermount3` 与 `/dev/fuse` 的用户态工具）。
4. 不需要 `winfsp`、`mounts3.dll`、`ZSpaceMount.exe`（Windows 版）随包。

---

## 4. 自测

```bash
native/mount/selftest/run_selftest.sh
```

覆盖「生成 conf → check → mount → 读 NAS 文件 → 写回 NAS → 隐藏文件过滤 →
rc status → shutdown → 卸载 → 清理状态文件 → disk --size」，
**16/16 通过**，证据在 `selftest/out/`（已 gitignore）。

自测用 `rclone serve webdav` 做 mock NAS，不接触真实 NAS、不使用账号凭据。

---

## 5. 依赖、版本与许可证

| 组件 | 版本 | 来源 | SHA256 | 许可 |
|---|---|---|---|---|
| rclone | v1.70.2 linux-amd64 | <https://downloads.rclone.org/v1.70.2/rclone-v1.70.2-linux-amd64.zip> | zip `7bfa813f426820d20436e48a4e0b9bf21977fcd513a48f2d28eede3136b4bacb`；二进制 `d1d99f58c1ad31592caa093eb400607725afd74eb3655bdeb6dc1886fed8fda8` | MIT |
| FUSE3 | 3.14.0-5deepin3（本机） | 发行版包 `fuse3` | — | libfuse LGPL-2.1；`fusermount3` GPL 部分（独立程序调用） |
| zspace-mount-monitor | 1.1.1-linux | 本仓库 | 见每次构建 `sha256sum` | MIT（本仓库） |

二进制不提交：rclone 放 `build/mount-runtime/`（`build/*` 已 gitignore），
监控器 `target/` 由 `native/mount/.gitignore` 排除。

---

## 6. 对齐与偏离

**对齐**：conf 文件名/字段/`[uzmount]`、`mount.port`/`mount.lock`、rclone 主要参数、
默认点文件过滤、`--show-hidden` 取消过滤、`--use-filter` 用 `filter.txt`、
缓存上限不足时取可用空间一半、`/mount/status`/`/shutdown` 语义、卸载后清理。

**偏离（均有原因）**：

1. 用上游 rclone 1.70.2 代替官方 fork：`/mount/*` 由监控器兼容，`--vfs-disk-space-info-from` 丢弃
   → 文件管理器里卷的容量可能显示为本地缓存盘而非 NAS 用量。
2. `--volname` 在 Linux 接受但无效果（rclone 仅 Win/mac 使用）。
3. `--network-mode`、`--no-fuse-mode` 接受但忽略（Windows 语义）。
4. 挂载点路径 `<cache-dir>/<volume-label>` 是从 mac 分支推导的，Linux 无官方定义。
5. 监控器日志子进程文件名用 `mount.log`，官方是 `mount_<pid>%03d.log` 轮转；JS 不读取该日志。
6. `fuse_status` 返回 `fuse3` 项，官方是 WinFsp/fuse-t 的结构；JS 只判断数组非空。
7. `rc /mount/status` 的 `is_mounting` 以「rclone rc 端口可连通」为准；
   官方 fork 返回的是真实挂载状态。上游 rclone 的 `mount/listmounts` 只列 rc 启动的挂载，
   对 CLI `rclone mount` 恒为空，无法用于判定。
8. 旧 `mounts3.dll` + S3 路径**未实现**：当前版本 `supportMountV2=true`，该路径不进入。
   若将来要兼容更老客户端（`webClientVersion < 2025070101`），需另做。

---

## 7. 待实测（需要真实 NAS/真机）

1. 真实 NAS 9001 代理是否对 `vendor=other` 的 WebDAV 客户端完整支持（读/写/重命名/删除）。
2. 大文件和视频的 Range/seek 是否正常（rclone VFS + WebDAV 的 Range 透传）。
3. 写入权限：NAS 侧 MinIO/文件分享服务是否允许 WebDAV PUT/DELETE。
4. 文件管理器容量显示：是否需要为 `--vfs-disk-space-info-from` 做替代（当前丢弃）。
5. Deepin 25 实机上 `fuse3` 是否默认安装、`/dev/fuse` 权限是否允许普通用户。
6. `--show-hidden` 在 UI 开关下是否与官方一致（本实现只验证了参数与过滤行为）。
7. 多账号切换：`home-dir` 隔离由 JS 提供，需确认退出登录/切换 NAS 时旧挂载被清理。
8. 旧客户端/旧固件（S3/mounts3 路径）是否需要兼容——当前版本的答案是不需要。

---

## 8. 文件

```
native/mount/
├── Cargo.toml
├── src/main.rs                 # zspace-mount-monitor
├── research/evidence.md        # 官方 CLI/JS 原始证据
├── selftest/run_selftest.sh    # 16 项端到端自测（mock WebDAV）
└── README.md
```
