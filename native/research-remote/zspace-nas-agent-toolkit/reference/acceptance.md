# 当前验收流程（0.3.11）

先读 `release-0.3.11.md` 与 `acceptance-remediation-0.2.3-rc.4.md`，再按其中引用读取既有业务与Windows写入处置记录。
以下历史记录只证明其标注版本/日期的行为，不能代替当前包复测。

## 本地

以下开发检查在源码包目录执行；运行包不包含构建和测试脚本。

```bash
scripts/check-security.sh
python3 scripts/release-manifest.py --verify
python3 scripts/validate-release.py
```

当前 catalog 为 91 项；风险数量和 raw 白名单以实际二进制输出为准。核对未知 action、合法只读动作、敏感输入、路径穿越、profile 隔离与注销失效，不只测试拒绝分支。

## 真机

1. Agent执行 `login --no-open --auth-code-stdin`，从stdout转交链接，并把用户回复的一次性码只写入当前进程stdin；不另开终端。
2. 默认 HTTPS 云连接执行 doctor，使用隔离 profile 抽样 file space/system info/file list。
3. 局域网仅使用可信 HTTPS --host；需要 HTTP 时每次明确指定 --host http://地址:端口 与 --allow-insecure-http。单独 --mode lan 不再执行不安全探测。
4. 仅在获准的专用目录做上传/下载/比对和清理；容器、网络等使用唯一临时对象，禁止修改既有资源。
5. 清理核验使用完整分页或精确 search，不以首页 50 条 grep 推断不存在；遍历分页并核对任务状态。
6. 专用验收 profile 完成后 logout 并确认 status.logged_in=false。不得输出或收集 token、授权码。

无有效授权时将业务真机复验标为待授权，不用内部口令或绕过鉴权冒充通过。以下记录为历史证据。

---

## 2026-10-10 真机验收记录

目标设备：`10.172.10.150`。验收使用专用 `acceptance` profile；一次性授权码仅经隐藏终端输入，
真实 Token 未进入命令行、标准输出或测试日志。验收结束后已执行 `logout`，并确认
`status.logged_in=false`。

| 范围 | 结果 |
| --- | --- |
| 自动寻址 | PASS；ZRP 返回的 LAN 地址经探测后选中 `http://10.172.10.150:5055` |
| 强制 LAN | PASS |
| 强制 cloud | PASS；经 `https://oauth.zconnect.cn` 与 `nas-id`/`remote-port` 访问 |
| OAuth 安全存储 | PASS；macOS Keychain，输出只含脱敏 NAS ID |
| `user_space` 固件兼容 | PASS；空 `device_dirs` 时推导并验证 1 个 `personal_spaces` |
| 文件闭环 | PASS；mkdir/write/info/list/read/rename/copy/move/download/upload/content verify |
| 只读业务抽样 | PASS；system、disk、storage、app、album、media、download、backup、ops、raw |
| 策略拦截 | PASS；无 `--yes` 写入、敏感参数、非白名单 raw 均在本地拒绝 |
| Linux 真机执行 | PASS；x86_64 musl 静态二进制、41 tools、3 destructive、profile 隔离 |
| Unix 权限 | PASS；`profiles/` 与 profile 目录 0700，配置/锁/文件凭证 0600 |
| 单元测试 | PASS；13/13 |
| Skill 校验 | PASS；skill-creator `quick_validate.py` |

专用 NAS 测试目录已从个人空间移入回收站（可恢复），没有清空用户的整个回收站。NAS `/tmp` 与本机
`/tmp` 中的验收二进制、临时 profile 和探针文件均已删除。

## 2026-10-10 v0.1.1 回归记录

根据 Windows/Git Bash 真机报告修复后，在同一 NAS（ZRP 选中 `http://10.172.10.114:5055`）使用专用
`acceptance-fix` profile 回归。一次性授权码只写入隐藏终端输入，OAuth 凭证保存于 macOS Keychain；
验收后已 `logout`，`status.logged_in=false`。

| 报告问题 | 回归结果 |
| --- | --- |
| 相对 `local_path` | PASS；CWD 在项目目录、`--local-root` 在 `/tmp` 时，相对路径上传/下载各 48 字节，SHA-256 一致 |
| `recyclebin list` | PASS；不传 `--yes` 即可读取 |
| Git Bash raw | Windows x64/ARM64 已交叉编译 MSYS 白名单后缀恢复逻辑；文档明确标签列表使用 `POST` |
| iSCSI / 音乐 502 | 根因确认；设备对两应用均返回 `is_installed:false,status:uninstalled`，命名命令现返回 `E_APP_NOT_INSTALLED` |
| 非 JSON 5xx | PASS；白名单 raw 直调未安装的 iSCSI 仍为 502，但错误已提示应用未安装/未启动/固件不支持 |
| `--host` / `--mode` | PASS；公网 host 在持久化前被拒绝，`--clear-host` 将保存值清为 `null`，`--mode auto` 恢复自动选路 |
| 失败下载残留 | PASS；真机返回 `N001315` 后，本地无目标文件且无 `.zspace-download-*.tmp` |
| profile 并发锁 | 当时仅验证锁冲突能映射为 `E_PROFILE_BUSY`；未覆盖多进程只读并发，后由 2026-10-12 Windows 报告发现不足 |

补充校验：auto/LAN/cloud 三通道的 `doctor` 均 PASS；19 项单元测试、Clippy `-D warnings`、
skill-creator 校验、6 个平台二进制校验和 ZIP 完整性检查均 PASS。专用验收目录
`/nvme19/my/data/_zspace_fix_v011_20261010` 已移入回收站，未清空回收站。

## 2026-10-12 v0.1.2 回归记录

依据 Windows v0.1.1 验收报告修复 profile 并发回归与下载 URL 校验缺口，并在同一 NAS 使用专用
`acceptance-v012-20261012` profile 验收。一次性授权码只写入隐藏终端输入，OAuth 凭证保存于 macOS
Keychain；验收后已执行 `logout` 并确认 `status.logged_in=false`。

| 报告问题 | 回归结果 |
| --- | --- |
| 并发只读 | PASS；`system info`、`storage info`、`album list`、`file space`、`app status` 三轮均为 5/5，stderr 为空 |
| 发布二进制并发 | PASS；新构建的 macOS ARM64 v0.1.2 再执行一轮混合并发，5/5 成功 |
| 无条件改写配置 | PASS；连续真机只读调用前后 `config.json` mtime 不变 |
| 新 profile 并发 | PASS；本地全新 profile 五进程同时初始化与 `status`，5/5 成功 |
| IPv4-mapped IPv6 | PASS；报告 R15/R18 均在本地返回 `E_POLICY`，未向 NAS 发出下载请求 |
| URL userinfo | PASS；报告 R17 返回 `E_POLICY`，未向 NAS 发出下载请求 |
| 其他主机表示 | PASS；单元测试覆盖整数 IPv4、尾点 localhost、IPv6 链路本地及大小写归一化 |
| FTP 协议 | 保留支持；NasServices 下载器与 workbuddy-connector 均明确将 FTP 列为受支持协议，FTP URL 仍执行同一主机安全检查 |
| 自动寻址 | PASS；选中 LAN `http://10.172.10.114:5055` |
| 强制 LAN | PASS |
| 强制 cloud | PASS；经 `https://oauth.zconnect.cn` 代理访问 |

实现侧将 profile 锁收敛到短时本地状态读写，锁竞争最多退避等待 3 秒；只在配置初次创建、显式连接
选项、凭证后端或 LAN 缓存发生变化时写入。Windows sharing/lock violation（OS error 32/33）统一映射为
`E_PROFILE_BUSY`。本地 22 项单元测试、Clippy `-D warnings`、Windows x86_64 交叉检查以及六平台发布
构建均通过。本次真机验收仅执行只读业务调用；SSRF 用例均在本地拦截，NAS 未产生测试下载任务。

## 2026-10-12 v0.1.3 回归记录

依据 v0.1.2 全量回归报告修复媒体契约、iSCSI 授权、参数校验与只读覆盖问题，并在同一 NAS 使用专用
`acceptance-v013-20261012` profile 验收。一次性授权码只经隐藏终端输入，OAuth 凭证保存于 macOS
Keychain；命令行、标准输出与测试日志均未记录授权码或 Token。

| 报告问题 | 回归结果 |
| --- | --- |
| M-1 空媒体列表为 `null` | PASS；音乐空库返回 `list:[]`、`count:0`、`empty:true` |
| M-2 列表到详情缺少 ID 映射 | PASS；视频实测列表为数组且条目带 `detail_id/detail_id_field`，顶层保留 `classification_id` |
| M-3 不存在的详情静默成功 | PASS；音乐无效 ID 返回 `E_NOT_FOUND`，并提示该媒体库需要的 ID 类型 |
| M-4 转码 action 无校验 | PASS；action 使用本地白名单，未知值返回 `E_PARAM`，不会请求应用接口 |
| I-1 Target/LUN 权限失败 | PASS；根因是 `zsanmanager` 将 `username` Cookie 与主账号比较，CLI 现从 OAuth userinfo 安全解析并按 profile 缓存账号名 |
| I-2 权限错误不可行动 | PASS；HTTP/业务权限失败统一为 `E_PERMISSION`，不再泄露内部模块名 |
| I-3 空态提示误报 | PASS；仅查询 sessions 时返回 `sessions:[]`，提示“未请求集合的状态未知” |
| D-I7 iSCSI raw 覆盖窄 | PASS；补充 info/storage、Target/LUN/Group 的安全只读详情与列表；CHAP 设置等敏感接口仍不在白名单 |
| X1 system fields 无校验 | PASS；非法 field 在本地返回 `E_PARAM` 并列出 8 个合法值 |
| X2 转码历史无法清理 | PASS（能力补齐）；后端源码确认 `/converter/v1/video/file` 的 `wipe` 可清空任务历史，CLI 将其标记为 destructive，并同时要求 `all=true` 与 `--yes`；后端仍不支持按单个 `task_id` 删除 |

iSCSI 真机只读验证结果：auto/LAN 通道返回 2 个 Target、1 个 LUN、1 个 Group、2 个存储项，服务状态
为 running，sessions 为空；强制 cloud 通道经 `https://oauth.zconnect.cn` 同样返回 2 个 Target、1 个 LUN。
响应中的 CHAP 密码字段保持 `[REDACTED]`。缓存账号名后的第二个进程仍能读取 Target/LUN，且调用前后
`config.json` 的 inode、mtime、ctime、大小与 SHA-256 均不变。

auto、强制 LAN、强制 cloud 三种 `doctor` 均 PASS；自动寻址选择
`http://10.172.10.114:5055`。本地 28 项单元测试、Rustfmt、Clippy `-D warnings`、release 构建及
skill-creator `quick_validate.py` 均通过；macOS、Linux、Windows 的 x86_64/ARM64 六个平台发布构建、
二进制 SHA-256 校验与 ZIP 完整性检查均 PASS。真机阶段只执行读取；未调用 `wipe`、未修改 iSCSI
配置，也未请求任何被排除的敏感 raw 接口。验收结束后已执行 `logout` 并确认
`status.logged_in=false`，随后删除本机临时 profile 目录。

## 2026-10-12 v0.1.4 回归记录

依据 v0.1.3 验收报告的 V 系列问题完成修复，并使用专用 `acceptance-v014-20261012` profile 验收。
一次性授权码只经隐藏终端输入，OAuth 凭证保存于 macOS Keychain；命令行、标准输出和日志均未记录
授权码或 Token。OAuth/ZRP 选中的 LAN endpoint 为 `http://10.172.10.114:5055`，auto、强制 LAN 与
强制 cloud 三种通道均 PASS。

| 报告问题 | 回归结果 |
| --- | --- |
| V1 上传冲突语义 | PASS；默认策略对已存在目标返回 `E_EXISTS`，不会静默产生副本；`overwrite:true` 原路径覆盖且远端内容/大小正确；显式 `rename:1` 返回 NAS 实际生成的 `payload-1.txt` 路径 |
| V2 零字节上传 | PASS；实际请求 NAS 并创建 0 字节文件；父目录不存在时返回后端错误；重复上传返回 `E_EXISTS`，不再假成功 |
| V3 参数解析输出 | PASS；未知选项、未知子命令和冲突参数均只输出一个 JSON `E_PARAM`，退出码为 2，且不回显输入值；`--help`/`--version` 保持人类可读且退出码为 0 |
| V4 iSCSI setting 口径 | PASS；文档明确 `/v1/zsanmanager/info` 允许返回脱敏设置摘要，而 `/setting/info` 不在白名单；真机响应中的 `password` 与 `mutual_password` 均为 `[REDACTED]` |
| V5 上传续传 | PASS；20,832,984 字节文件在首个 8 MiB 分块后中断，再次上传读取到 `remote_partial_size=8,388,608`，从同一偏移继续且只传剩余 12,444,376 字节；完成后回读 SHA-256 与本地一致 |
| V6 转码 create 校验 | PASS；缺少 `source_path` 或源文件不存在均在创建前返回 `E_PARAM`，真机任务列表未新增任务；成功路径的 create→file 两阶段流程、加文件失败的 `E_CONVERTER_ADD_FILE_FAILED` 与保留 `task_id` 均有 HTTP 契约测试覆盖 |
| V8 账号名错误分类 | PASS；成功 userinfo 但无可解析账号名时返回 `E_CREDENTIAL`；NAS 403 仍返回 `E_PERMISSION`，且不会再误触发 Token 刷新 |

真机写入均位于 `/nvme19/my/data/_zspace_v014_acceptance_20261012_01`。验收结束后该目录已移入
回收站并确认原路径不存在，没有清空回收站；既有转码任务 31 未被修改，也未执行全局 `wipe`。随后
执行 `logout` 并确认 `status.logged_in=false`，删除本机临时 profile 与回读文件。

本地 42 项单元测试与 2 项 CLI 进程契约测试、Rustfmt、Clippy `-D warnings`、release 构建及
skill-creator `quick_validate.py` 均通过。macOS、Linux、Windows 的 x86_64/ARM64 六个平台发布构建、
二进制 SHA-256 与 ZIP 完整性检查均 PASS，产物为 `zspace-nas-agent-toolkit-v0.1.4.zip`。其中 Linux
x86_64 musl 静态二进制还在 `10.172.10.150` 真机执行了格式、校验和、版本及 JSON 参数错误契约检查。
