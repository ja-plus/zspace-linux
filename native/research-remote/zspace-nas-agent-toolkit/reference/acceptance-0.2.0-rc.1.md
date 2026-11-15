# 0.2.0-rc.1 阶段性功能验收版

日期：2026-10-16（Asia/Shanghai）。本版收束当前功能实现，暂停继续扩展，供测试验收。
全部业务域尚未完成；14 项安全整改按用户要求暂缓。历史 0.1.5-rc 安全验收材料描述其他实现，
不适用于本包，未纳入本 ZIP。不要据此认定 LAN、下载等入口已具备那些历史材料中的安全限制。

## 安装与核验

解压 ZIP，保留完整的 `zspace-nas-agent-toolkit/` 目录。macOS/Linux 使用
`scripts/zspace-cli`；Windows 使用 `scripts/zspace-cli.ps1` 或 `.cmd`。
本包包含 macOS、Linux、Windows 的 ARM64 与 x64 六种二进制；Linux 使用静态 musl。

```sh
# 在解压目录中执行
scripts/zspace-cli --version
# 预期：zspace-cli 0.2.0-rc.1
(cd bin && shasum -a 256 -c SHA256SUMS)
scripts/zspace-cli catalog
```

Windows 可以用 `Get-FileHash -Algorithm SHA256` 对照 `bin/SHA256SUMS`。
ZIP 外部另附 SHA-256 校验文件。哈希用于完整性检查，不是发布者签名。
运行时包内 `release-manifest.json` 记录源码快照、平台架构和二进制哈希。
未进行代码签名/公证；Windows 与非本机架构仍需原生设备验收，以包验证记录为准。

## 本版范围

命名工具共 91 项，分组 action 的实际参数见 `reference/function-expansion.md` 与 `tool-catalog.md`。
本轮扩展包含应用/Docker/Compose、文件归档和元数据、笔记/收集/分享、保险箱/快照、
网盘传输/备份/Syncthing、音乐/有声书/阅读/漫画、相册组织/分享/EXIF/导出、
影视自定义专辑/字幕/播放与记录、虚拟机/iSCSI、查杀、网络与系统设置、USB模式、
Time Machine、浏览器应用及本地 AI 等；各 action 的完成和验证程度不同。
不能用工具数量代替业务完成率。

已执行的真机测试见 `acceptance-functions-2026-10-16.json`；全量范围及未完成项见
`coverage-functions-2026-10-15.md`。真机证据来自同批功能源码的既有测试，并非本次重新执行全部写入。
打包后的本机与 NAS 二进制冒烟结果另见 `package-validation-0.2.0-rc.1.json`。

主要边界：影视封面上传/NAS选图/URL下载、完整刮削/智能跳过/共享直播审阅、
多项跨设备备份/恢复、Docker终端/事件/镜像导出、VM硬件直通、iSCSI迁移/克隆/导入、
远程访问、部分独立应用及完整 AI 助手/知识库仍待开发或验证。
Time Machine 无专用 Mac，未做实际备份；GPU接口在当前设备不可用；
防火墙及现网重配未进行真实写入；不会把这些项目标为通过。

## 建议验收顺序

1. 核对版本、平台和哈希，执行 `--help`、`catalog`。
2. 建立独立 profile，例如 `--profile acceptance-020rc1`，通过交互式 `login` 隐藏输入授权码；
   核对身份，执行 `status`、`doctor`。不要沿用开发机的测试凭证。
3. 先验收只读命令，再创建独立 NAS 测试目录及应用对象，记录对象 ID。
4. 按相应入口与功能契约验证 CRUD、任务终态、内容往返及清理；不能只看提交成功。
5. 影视可用自有短片验证音轨/字幕、播放资源与记录；EXIF 使用可丢弃的 JPEG，日期/GPS会修改原图。
   旋转写入 NAS 展示方向，不承诺改变原文件 Orientation 标签。
6. 写操作需明确目标与影响，按命令要求提供 `--yes`、确认字段及最新 `expected_*` 数据。
   避免用现有用户数据测试恢复、初始化、格式化、全局替换、AI重建或共享通知。
7. 删除仅限本次测试对象；不清空全局回收站。结束执行 `logout`，确认 `status.logged_in=false`。

本地文件传输须指定 `--local-root`。异步扫描/刮削/安装/生成任务应查询终态，超时不要重复提交。
测试反馈请包含版本、平台/架构、脱敏命令、预期/实际结果和错误码；不要提交授权码、Token、
profile文件、播放/分享能力链接、保险箱PIN或业务口令。
