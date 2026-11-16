# 登录后实测步骤清单（原生版 native.6）

> 适用包：`com.zspace.native.spark_2.46.2026101501+native.6-1_amd64.deb`
> （native.6 以实际产物为准；命令里的文件名按你手上的包替换。）
> 本文所有路径/日志字段来自当前代码与实机日志，不依赖 Wine。

## 0. 路径速查

| 项 | 路径 |
|---|---|
| 安装目录 | `/opt/durapps/com.zspace.native.spark/` |
| 启动器 | `/usr/bin/com.zspace.native.spark`、`/opt/durapps/com.zspace.native.spark/files/run.sh` |
| 用户数据 | `~/.config/zspace/` |
| 主日志 | `~/.config/zspace/logs/main.log`（轮转 `main.old.log`） |
| 播放器日志 | `~/.config/zspace/logs/player.log` |
| 挂载日志（主进程侧） | `~/.config/zspace/logs/mount.log` |
| libzrpc 日志 | `~/.config/zspace/proxy/ZSpace/proxy/log.txt` |
| 同步日志 | `~/.config/zspace/<用户名>[_<nasId>]/.config/syncthing.log` |
| 挂载 home | `~/.config/zspace/<用户名>[_<nasId>]/mount/`（`mount.conf`/`mount.port`/`mount_monitor.log`） |

同步/挂载目录名：`zspace` 且用户名以数字开头时是 `<用户名>`；否则 `<用户名>_<nasId>`。

```bash
UD="$HOME/.config/zspace"
ls -d "$UD"/*/ 2>/dev/null
```

---

## 1. 安装前

### 1.1 安装命令（优先 apt）

```bash
cd <deb 所在目录>
sha256sum com.zspace.native.spark_2.46.2026101501+native.6-1_amd64.deb
sudo apt install ./com.zspace.native.spark_2.46.2026101501+native.6-1_amd64.deb
```

- 用 `apt install ./xxx.deb`：会自动解析 `Depends`（含 `fuse3`、Electron 运行库）
  并安装 `Recommends`（VLC/VA-API）。
- 如果先用了 `dpkg -i` 报缺依赖，再补：

```bash
sudo dpkg -i com.zspace.native.spark_2.46.2026101501+native.6-1_amd64.deb
sudo apt -f install
```

- Deepin 25 的 t64 包名（`libgtk-3-0t64` 等）已在 `Depends` 里用 `|` 兼容。
- 播放需要 VLC 运行库；若 `Recommends` 被 `--no-install-recommends` 跳过：

```bash
sudo apt install libvlc5 vlc-plugin-base vlc-plugin-video-output \
  libva2 libva-drm2 libva-x11-2
```

### 1.2 确认装对了

```bash
dpkg -s com.zspace.native.spark | grep -E 'Status|Version|Depends'
dpkg -L com.zspace.native.spark | head -30
test -x /opt/durapps/com.zspace.native.spark/files/run.sh
test -x /opt/durapps/com.zspace.native.spark/files/electron/electron
test -x /opt/durapps/com.zspace.native.spark/files/app/uzplayer/zspace-player
test -x /opt/durapps/com.zspace.native.spark/files/app/bin/syncthing
test -x /opt/durapps/com.zspace.native.spark/files/app/bin/ZSpaceMountMonitor
test -x /opt/durapps/com.zspace.native.spark/files/app/bin/ZSpaceMount
command -v fusermount3
```

预期：
- `Status: install ok installed`
- `Depends` 里能看到 `fuse3`
- 上面所有 `test -x` 无输出（成功）
- `fusermount3` 有路径

不符合时记下：`dpkg -s` 完整输出、`apt` 的依赖报错原文、`dpkg -L` 里缺的文件。

---

## 2. 第一次启动

### 2.1 启动

菜单里点「极空间」，或：

```bash
com.zspace.native.spark
# 第一次排查建议直接跑；stderr 会留在终端
bash /opt/durapps/com.zspace.native.spark/files/run.sh
```

### 2.2 预期

- 出现登录窗口（不是 Wine 窗口），标题/界面正常，无黑屏。
- 主日志出现：

```bash
tail -n 100 ~/.config/zspace/logs/main.log
```

关键字（顺序大致如下）：
- `Main window loaded:app://./index.html#/login`
- `VLC video player path:.../uzplayer/zspace-player`
- `PlayerServer listen on port:`
- `load dll success`
- `Proxy SDK inited`

### 2.3 起不来时先看哪几行

```bash
grep -nE 'Error|ERROR|Cannot find module|ENOENT|libzrpc|player|VLC' \
  ~/.config/zspace/logs/main.log | tail -50
```

- 空白/闪退：终端里 `run.sh` 的 stderr、`main.log` 末尾 50 行。
- `libzrpc` 加载失败：`ls -l .../files/app/sdk/libzrpc.so`、`file` 确认 ELF。
- 播放器缺失：`ls -l .../files/app/uzplayer/zspace-player`。
- 图形问题：`echo $DISPLAY $WAYLAND_DISPLAY`，并附 `main.log` 截图。

---

## 3. 登录

### 3.1 方式

- **账号登录（normal，推荐先测）**：走云端拿 nasId → 本地代理 → 中继隧道。
- **IP 直连（ipv4/ipv6/local）**：填 NAS 的 `IP:5055`（或 IPv6），走局域网直连。

### 3.2 重点：局域网扫描面板是否出现

预期：**账号登录成功后不再出现「局域网设备扫描 / 没有在局域网中发现设备」。**

代码侧原因：该面板只在三处失败时出现——云端 `/key/pub(server)`、云端
`/quick/nasid`、NAS `/key/pub`。其中 `/key/pub` 在登录 POST 之前，失败会直接
短路进面板；所以只要能登录成功，`/key/pub` 必然已成功。

核对 `/key/pub` 是否 200：

```bash
# 找本地代理端口（simpleStartSerivce 默认 13579，占用时会 +1）
ss -ltnp | grep 127.0.0.1

PORT=13579   # 按 ss 结果改成实际的
curl -s -o /dev/null -w '%{http_code}\n' \
  "http://127.0.0.1:$PORT/key/pub?signme=1"
```

预期：`200`。再确认返回体是 JSON（不是 `Connection refused`/`403`）。

### 3.3 失败时抓什么

```bash
grep -nE 'simpleStartSerivce|start sservice for login|libzrpc|key/pub|getPublicKey' \
  ~/.config/zspace/logs/main.log | tail -50
tail -n 80 ~/.config/zspace/proxy/ZSpace/proxy/log.txt
ss -ltnp | grep 127.0.0.1
```

- 面板出现时：截面板图 + 上面三段日志。
- `/key/pub` 非 200：记录 HTTP 码、`curl -v` 输出、`proxy/.../log.txt` 末尾。
- 云端失败（VPN/代理/断网）：先断开系统代理再试；记录 `main.log` 里
  `get server public key error` / `get nasId error`。

---

## 4. 登录后逐项验证

### 4.1 远程隧道是否通（tunnel vs direct）

先看 SDK 日志：

```bash
SDK="$HOME/.config/zspace/proxy/ZSpace/proxy/log.txt"
grep -nE 'start_service: relay tunnel|connect ok via|proxy 127\.0\.0\.1' "$SDK" | tail -30
```

- `start_service: relay tunnel 127.0.0.1:<port> -> nas "<id>"` = **远程中继**
- `proxy 127.0.0.1:<port> -> https://<ip>:5055` = **局域网直连**
- 远程时还应看到 `connect ok via <relay>:9091`

辅助：

```bash
grep -nE 'relay tunnel|start sservice for login|conn_type|local_network' \
  ~/.config/zspace/logs/main.log | tail -20
```

预期：账号登录且 NAS 不在同一局域网时，是 `relay tunnel`；IP 直连时是
`proxy ... -> https://<ip>:5055`。

不符合时记下：上面 grep 的完整输出、`ss -ltnp`、NAS 是否在线、是否同一局域网。

### 4.2 视频播放

操作：打开一部 NAS 上的视频（本地/远程都可）。

主日志：

```bash
grep -nE 'VLC video player path|Run player by cmd|PlayerServer listen|Player server been connected' \
  ~/.config/zspace/logs/main.log | tail -20
```

播放器日志：

```bash
tail -f ~/.config/zspace/logs/player.log
```

预期：
- `VLC video player path:.../uzplayer/zspace-player`
- `Run player by cmd:"...zspace-player" ...`
- `Player server been connected`
- `player.log`：`已连接主进程` → `收到 playList` → `开始播放`
- 画面正常；进度条走动；`player.log` 周期性出现 `发送 4/3`（进度上行）
- 关闭播放器后出现 `发送 1/3`（停止）

不符合时记下：`main.log` 的 `Open player error`/40s 超时提示、
`player.log` 末尾 80 行、`pgrep -af zspace-player`、`ls -l /dev/dri`（硬件解码）。

### 4.3 同步

操作：设置 → 同步（mountV2/同步界面）；看设备/目录是否正常显示。

主日志：

```bash
grep -nE 'Sync service:|local sync api port|sync service start|syncthing' \
  ~/.config/zspace/logs/main.log | tail -30
```

定位同步配置并直接验 REST：

```bash
UD="$HOME/.config/zspace"
CFG=$(find "$UD" -maxdepth 4 -path '*/.config/config.xml' | head -1)
echo "$CFG"
PORT=$(grep -oP '(?<=<address>127\.0\.0\.1:)\d+' "$CFG" | head -1)
KEY=$(grep -oP '(?<=<apikey>)[^<]+' "$CFG")
echo "port=$PORT"
curl -s -o /dev/null -w 'system=%{http_code}\n' \
  -H "X-API-Key: $KEY" "http://127.0.0.1:$PORT/rest/system/status"
curl -s -o /dev/null -w 'nokey=%{http_code}\n' \
  "http://127.0.0.1:$PORT/rest/system/status"
```

预期：
- `syncthing` 进程在跑：`pgrep -af 'bin/syncthing'`
- `system=200`、`nokey=403`
- 同步界面能打开，设备/目录列表正常
- 同步日志无持续报错：

```bash
tail -n 80 "$(dirname "$CFG")/syncthing.log"
```

不符合时记下：`main.log` 的 `sync exe file missing`/`initConfig error`/
`sync xml file error`、`syncthing.log` 末尾、`ss -ltnp | grep "$PORT"`。

### 4.4 挂载

操作：设置 → 挂载（mountV2）；选缓存目录与盘符/卷标，点挂载。

```bash
grep -nE 'mount|Mount' ~/.config/zspace/logs/main.log | tail -40
tail -f ~/.config/zspace/logs/mount.log
pgrep -af 'ZSpaceMountMonitor|ZSpaceMount'
```

挂载 home：

```bash
MH=$(dirname "$(find "$HOME/.config/zspace" -maxdepth 3 -name mount.conf | head -1)")
echo "$MH"
ls -la "$MH"
cat "$MH/mount.port" 2>/dev/null
tail -n 80 "$MH/mount_monitor.log"
```

预期：
- 进程：`ZSpaceMountMonitor` 与 `ZSpaceMount` 各一个（`comm` 会截断为
  `ZSpaceMountMoni` / `ZSpaceMount`，`pgrep -f` 可看全路径）
- 卷/目录出现：`findmnt | grep -i '极空间\|zspace'`
- 挂载点（Linux 按 mac 规则推导）：`<缓存目录>/极空间-个人`
- 读写测试：

```bash
MP="<缓存目录>/极空间-个人"
echo "checklist $(date)" > "$MP/checklist-test.txt"
cat "$MP/checklist-test.txt"
rm "$MP/checklist-test.txt"
```

- UI 点卸载后：进程消失、`findmnt` 无该挂载点、`mount.port`/`mount.lock` 被清掉。

不符合时记下：`mount.log`、`mount_monitor.log`、`findmnt` 输出、
`ls -l /dev/fuse`、`fusermount3 -V`、`$MH/mount.conf`（**把 pass 打码**）。

---

## 5. 每项的「不符合预期」记录模板

| 项 | 记录内容 |
|---|---|
| 安装 | `dpkg -s` 全文、`apt` 报错、缺的依赖 |
| 启动 | 终端 stderr、`main.log` 末尾 50 行、截图 |
| 登录/面板 | 面板截图、`main.log` 的 `simpleStartSerivce`/`key/pub` 段、`curl -v` |
| 隧道 | SDK `log.txt` 的 `relay tunnel`/`connect ok`/`proxy` 段、`ss -ltnp` |
| 播放 | `main.log` 的 `Run player by cmd`/`Open player error`、`player.log` 末尾、进程 |
| 同步 | `main.log` 的 `Sync service` 段、`syncthing.log` 末尾、REST 两个状态码 |
| 挂载 | `mount.log`、`mount_monitor.log`、`findmnt`、`/dev/fuse`、`mount.conf`（打码） |

建议统一存到 `~/zspace-test-YYYYMMDD/`，文件名带时间戳。

---

## 6. 收尾：卸载与残留检查

### 6.1 卸载

```bash
# 保留用户数据（配置/日志还在）
sudo apt remove com.zspace.native.spark

# 连用户数据一起清（推荐测完用）
sudo apt purge com.zspace.native.spark
```

- 两种方式都会执行 `postrm`，清理 `~/.deepinwine/com.zspace.native.spark`
  （星火红线；原生版实际不用 Wine，但保留）。
- `purge` 还会删：`~/.config/zspace`、`~/.config/ZSpace`、`~/.config/极空间`、
  `~/.cache/zspace`、`~/.cache/ZSpace`、`~/.cache/极空间`、
  `~/.local/share/zspace`、`~/.local/share/ZSpace`。

### 6.2 确认没有残留

```bash
dpkg -l | grep com.zspace.native.spark || echo "package removed"
ls -d ~/.config/zspace ~/.deepinwine/com.zspace.native.spark 2>/dev/null || echo "dirs clean"
pgrep -af 'electron|zspace-player|ZSpaceMount|bin/syncthing' || echo "no processes"
findmnt | grep -i '极空间\|zspace' || echo "no mounts"
```

若还有挂载残留：

```bash
findmnt | grep -i '极空间\|zspace'
fusermount3 -u "<残留挂载点>"
```

若还有进程：

```bash
pkill -f '/opt/durapps/com.zspace.native.spark/'
```

预期：包已移除、`~/.config/zspace` 与 `~/.deepinwine/...` 不存在（purge 后）、
无相关进程、无相关挂载点。
