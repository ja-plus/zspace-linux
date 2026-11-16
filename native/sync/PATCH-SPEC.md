# 同步（syncthing）接入规格

- 官方 `ZSpaceSync.exe` = syncthing **v1.27.7** 的 fork
  （`bin/version.txt`: `v1.27.7-xnas-c20260624135721`）。
- 我们使用上游同版本 Linux amd64 二进制，REST / `X-API-Key` 与上游一致。
- 二进制不入库：`native/work/syncthing/syncthing`，用
  `scripts/fetch_syncthing.sh` 下载（sha256 固定）。

## 1. background.js 真实行为（已核对）

- 二进制路径：
  `this.binPath = path.join(__dirname, "bin", nt).replace("app.asar","app.asar.unpacked")`
  （`nt` 在非 macOS 上为 `ZSpaceSync.exe`）。
- 配置目录：`<userData>/<username>[_<nasId>]/.config`
  （`zspace` 且 username 以数字开头时直接用 username）。
- 配置/日志：`config.xml`、`syncthing.log`。
- 端口：GUI `8384`、数据 `22000`；`getPortAndKey()` 会从这两个值开始
  探测空闲端口，并把 config.xml 改写为
  `<address>127.0.0.1:<gui></address>` 和
  `<listenAddress>tcp://127.0.0.1:<data></listenAddress>`（仅第一条）。
- API key：`md5(username + "zspace")` 前 16 位，由 `initConfigContent()`
  写入 `<apikey>`，`getPortAndKey()` 再读出，存到
  `app.syncConfig.localApiPort/localApiKey`。
- 首次生成配置（官方调用）：
  `ZSpaceSync.exe -no-browser -generate=<configPath>`（环境 `STNODEFAULTFOLDER=1`）。
- 启动（官方调用，实机进程确认）：
  ```
  ZSpaceSync.exe --no-browser --home=<configPath> --logfile=<configPath>/syncthing.log \
    --log-max-old-files=1 --log-max-size=52428800 --allow-newer-config
  ```
  环境额外设置 `STTRACE=model versioner fs api`。
- 渲染器本地同步 API：`http://127.0.0.1:<localApiPort><path>`，
  头 `X-API-Key: <localApiKey>`。
- `syncstate.exe` 仅 `win32` 分支使用，Linux 不涉及。

## 2. 补丁（接在 `native/patches/patch_app.py`，幂等）

| 名称 | 原文/锚点 | Linux 替换 |
|---|---|---|
| sync-bin | `this.binPath=U.a.join(__dirname,"bin",nt).replace("app.asar","app.asar.unpacked");` | 追加 `"linux"==process.platform&&(this.binPath=U.a.join(__dirname,"bin","syncthing"));` |
| sync-generate | `["-no-browser","-generate="+this.configPath]` | `"linux"==process.platform?["generate","--home="+this.configPath]:["-no-browser","-generate="+this.configPath]` |
| sync-log-date | `/^(\d+\/\d+\/\d+ \d+:\d+:\d+)/` | `/^(\d+[\/-]\d+[\/-]\d+ \d+:\d+:\d+)/`（上游日志是 `YYYY-MM-DD`） |
| sync-kill-linux | ``t=Ze?`killall -9 "${nt}"`:`taskkill /f /im "${nt}"` `` | Linux：``pkill -9 -f "[s]yncthing --no-browser --home=${this.configPath}"`` |

`sync-kill-linux` 的原因：官方只会在 Windows/macOS 上杀 `ZSpaceSync.exe`，
Linux 下 `taskkill` 不存在、`killall ZSpaceSync` 也匹配不到 `syncthing`，
旧进程残留会让下一次启动把 GUI 端口 +20000（8384→28384）并因 leveldb
被占用而启动失败，托盘弹窗停在「本地服务未启动」。按 `--home` 配置路径
匹配，可以同时清掉上次运行留下的残留实例。

`sync-generate` 的原因：上游 Linux 不接受单横线长参数
（`-no-browser` 会被当成 `-n`），生成配置要用 `generate --home=...`。
启动参数本身是双横线，无需改。

另外，`libzrpc_start_service` 的 `code` 必须是**字符串** `"200"`：
同步线 `Fe.startService()` 用严格比较 `"200"===o.code`，数字 200 会让
`remoteSyncPort` 变成 undefined，配置里写成
`tcp://127.0.0.1:undefined`，同步设备永远连不上。

## 3. 安装位置

- `patch_app.py` 第 4 个参数：syncthing 二进制 → `APP/bin/syncthing`（0755）。
- `native/run-native.sh`：默认用 `native/work/syncthing/syncthing`，缺失只告警。
- `scripts/build_native_deb.sh`：缺失直接失败；新增自检
  A20（随包可执行）、A21（ELF）、A22（sync 补丁标记）。

## 4. 本地验证（已做）

按官方参数在 `/tmp` 起上游 1.27.7（GUI 18384/数据 12200，避开已装官方客户端）：

```
syncthing generate --home=<cfg>          # STNODEFAULTFOLDER=1
syncthing --no-browser --home=<cfg> --logfile=<cfg>/syncthing.log \
  --log-max-old-files=1 --log-max-size=52428800 --allow-newer-config
```

- `GET /rest/system/status` + `X-API-Key` → 200；不带 key → 403。
- `POST /rest/config/folders` 新增测试文件夹 → 200；
  `GET /rest/db/status?folder=test` → 200（`errors/globalFiles/...`）。
- 按 `initConfigContent` 的改写（GUI 用户、key、关闭 announce/relay/nat 等）
  与 `md5(username+"zspace")[:16]` key 再跑一次，同样 200/403 符合预期。

## 5. 待实测

- 官方同步界面的**登录后**联调：界面在 NAS web 里，没有登录态无法完整点开；
  本地已按同一 `X-API-Key` + 端口契约验证。
- 与真实 NAS 的同步目录/设备握手（需要 NAS 与登录态）。
- `native/mount/` 那条线的 rclone 挂载不在本规格内。
