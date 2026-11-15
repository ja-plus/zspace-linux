# background.js 播放器补丁规格（Linux）

- 目标：让官方主进程在 Linux 上拉起 `zspace-player`，替代 `UZPlayer.exe`。
- 口径：只改「播放器」相关分支，不新增官方没有的选项/兼容层；
  不改官方二进制、不改渲染器。
- 实施位置：`native/patches/patch_app.py`（**本文只给规格，不改该文件**）。
- 取证：`resources/app.asar` 解包后的 `background.js`（webpack 压缩，单行）
  + `main.old.log` / `player.log` 实跑日志。
- 行号不可靠（压缩后单行），所有锚点都用**唯一字符串**定位，已逐一核对出现次数 = 1。

---

## 0. 路径与安装

设 `APP` = `background.js` 所在目录（当前原生版是 `native/work/app/extracted`）。

官方原逻辑：

```js
Pa = Ra ? "UZPlayer.app/Contents/MacOS/UZPlayer" : "uzplayer\\UZPlayer.exe"
za = ka.join(__dirname, Pa);
za = za.replace("app.asar", "app.asar.unpacked");
```

- 官方 Windows：`<resources>/app.asar.unpacked/uzplayer/UZPlayer.exe`
- 当前原生版（目录布局，没有 app.asar）：`za` 的 `replace` 不生效，
  路径是 `<resources>/app/uzplayer/UZPlayer.exe`
- Linux 目标：**`APP/uzplayer/zspace-player`，权限 0755**

```bash
install -d "$APP/uzplayer"
install -m 0755 native/player/target/release/zspace-player "$APP/uzplayer/zspace-player"
```

> 注意：`uzplayer/` 在精简后的 `native/work/app/extracted` 里可能不存在
> （当前 58 MB 的解包里没有播放器目录），需要从完整 asar.unpacked 重新取，
> 或只放我们的二进制 + 复用官方 `title`、`NotoSansSC.ttf` 等素材（可选）。

---

## 1. 播放器路径（`Pa` / `za`）

**原文**（约 4,747,063 字符处）：

```js
Pa=Ra?"UZPlayer.app/Contents/MacOS/UZPlayer":"uzplayer\\UZPlayer.exe",La="",Ma=!0,Ua=0,Ba=["OP","FHD_EX","HD","SD"],Fa=0,ja=!1,za=ka.join(__dirname,Pa);za=za.replace("app.asar","app.asar.unpacked");
```

**补丁**（在 `za=za.replace(...)` 之后追加一条 Linux 覆盖，避免动 `Pa` 的反斜杠转义）：

- 锚点（唯一）：`za=za.replace("app.asar","app.asar.unpacked");`
- 替换为：

```js
za=za.replace("app.asar","app.asar.unpacked");"linux"==process.platform&&(za=ka.join(__dirname,"uzplayer/zspace-player"));
```

**为什么不动 `Pa`**：

- `Pa` 还用于 `gc()` 推导进程名（`Pa.split(/[/\\]/).pop()`）；`gc()` 我们单独打补丁（第 3 条）。
- 只覆盖 `za` 可把改动面缩到最小，Windows/mac 分支完全不变。

**边界**：

- `ka` 是 `path` 模块；`join` 会规范分隔符，Linux 用 `/` 正确。
- 若 `APP` 路径含空格/中文：`za` 是字符串，后续启动命令已加双引号（`"${za}"`），不受影响。
- 幂等：第二次执行时 `za=...zspace-player...` 已存在，补丁应检测
  `"zspace-player"` 是否已出现再决定是否追加。

---

## 2. `VlcPreload.exe` 预热（`Qa`）

**原文**（约 4,747,800 字符处）：

```js
Qa=()=>{let e=i.app.getName().toLowerCase(),t=ka.join(ka.dirname(za),"VlcPreload.exe");r.a.info("VlcPreload path :"+t);let n=(new Date).getTime();Ea(`"${t}" ${e}`,(e,t)=>{let o=(new Date).getTime();e?r.a.info(`Run VlcPreload error in ${o-n} ms: `,e):(global.VlcPreloaded=!0,r.a.info(`Run VlcPreload end in ${o-n} ms`))})}
```

**补丁**：

- 锚点（唯一）：`Qa=()=>{let e=i.app.getName().toLowerCase()`
- 替换为：

```js
Qa=()=>{if("linux"==process.platform)return void(global.VlcPreloaded=!0);let e=i.app.getName().toLowerCase()
```

**理由**：Linux 没有 `VlcPreload.exe`；官方在 Windows 用它预热 VLC 插件缓存，
Linux 由 libvlc 自己扫插件（首次启动略慢，后续正常）。

**边界**：

- 只设置 `VlcPreloaded=true`，`Qa` 的其他逻辑（mac 的 unzip/拷贝）不执行。
- 如果 `za` 指向的目录没有 `VlcPreload.exe`，不 patch 就会每 180 s 报一次
  `Run VlcPreload error`（非致命，但会持续刷日志）。

---

## 3. 关闭/复用播放器时的 kill 命令（`gc`）

**原文**（约 4,781,344 字符处）：

```js
let t,n=Pa.split(/[/\\]/).pop();t=Ra?`killall -9 "${n}"`:`taskkill /f /im "${n}"`,r.a.info(t);
```

**补丁**：

- 锚点（唯一）：``t=Ra?`killall -9 "${n}"`:`taskkill /f /im "${n}"` ``
- 替换为：

```js
t=Ra?`killall -9 "${n}"`:"linux"==process.platform?"pkill -x zspace-player":`taskkill /f /im "${n}"`
```

**说明**：

- 这里用字面量 `zspace-player`，不依赖 `Pa`（`Pa` 仍是 `UZPlayer.exe`）。
- Linux 进程名（comm）最长 15 字符；`zspace-player` 是 13 字符，`pkill -x` 可精确匹配。
- `gc()` 的调用被 `try/catch` 包住，没有进程时 `pkill` 返回 1 不会中断主流程。
- 官方在启动前会先 `gc()` 杀旧播放器，再拉起新的；补丁后行为一致。

**边界**：

- 若系统没有 `pkill`（极简容器）：启动前杀不掉旧实例，新实例连接同一端口也能工作，
  但两个窗口会并存。可回退到 `killall -9 zspace-player`（本机 procps 提供 `pkill`）。
- 外部播放器（`localPlayerType` 设置）的 kill 命令在另一处（变量 `o`），
  不属于本次 6 处；Linux 上第三方播放器设置仍会走 `taskkill`，需要另开补丁。

---

## 4. 启动即视为 VLC 已就绪（`global.VlcPreloaded`）

**原文**（约 4,820,871 字符处）：

```js
global.VlcPreloaded=!1,"darwin"==process.platform?global.VlcPreloaded=!0:Object(Rc.setTimeout)(()=>{global.VlcPreloaded=!0},18e4)
```

**补丁**：

- 锚点（唯一）：`"darwin"==process.platform?global.VlcPreloaded=!0:Object(Rc.setTimeout)`
- 替换为：

```js
"darwin"==process.platform||"linux"==process.platform?global.VlcPreloaded=!0:Object(Rc.setTimeout)
```

**理由**：`yc()` 开头有硬门槛：

```js
if(!global.VlcPreloaded)return ...toastMessage("首次使用，加载播放器需要一定时间...")
```

Windows 要等 `VlcPreload.exe` 成功、mac 直接 true；Linux 没有预热步骤，
必须在启动时就置 true，否则点播放只会弹 toast。**保留 `yc()` 里的判断不动**，
只改初始化赋值，行为最小。

**边界**：Windows 原 180 s 兜底不受影响；mac 逻辑不变。

---

## 5. 音乐 SDK 路径（`playsdk.dll`）

**原文 A**（约 4,594,770 字符处，`vlcPlayerReady` 回调）：

```js
Rt?(Ot=Tt.join(t,"sdk","libplaysdk.1.0.0.dylib"),xt=Tt.join(n,"UZPlayer.app/Contents/Frameworks/"),Dt=Tt.join(o,"Contents/Frameworks/")):(Ot=Tt.join(e,"playsdk.dll"),xt=Tt.dirname(Ot),Dt="")
```

**补丁 A**：

- 锚点（唯一）：`:(Ot=Tt.join(e,"playsdk.dll"),xt=Tt.dirname(Ot),Dt="")});`
- 替换为：

```js
:"linux"==process.platform?(Ot="",xt="",Dt=""):(Ot=Tt.join(e,"playsdk.dll"),xt=Tt.dirname(Ot),Dt="")});
```

**效果**：Linux 上 `Ot=""`，音乐入口 `Bt()` 的第一道判断直接返回：

```js
if(!Ot)return Ut({code:1e3,msg:"音乐播放器未初始化，请稍后重试"})
```

即「音乐播放在原生版不可用」，但不会卡死、不会尝试 `kernel32`/`dlopen playsdk.dll`。
`Bt()` 里后面的 `St.Library("kernel32",...)` 在 `!Ot` 提前返回之后，不会执行，
所以**不需要**再改 kernel32 那段。

**为什么不实现 `libplaysdk.so`**：19 个 C 导出 + 均衡器/Dolby/声道，
属于独立工程；MVP 口径是「视频播放器先可用」。这是明确偏离，需在发布说明中标注。

**边界**：

- `global._musicDll` 在 Linux 上恒为空，不影响视频。
- 若以后实现 `libplaysdk.so`，把 `Ot` 指向 `.so` 并补上 `dlopen` 分支即可，
  补丁 A 可回退成 `"linux"==process.platform?(Ot=Tt.join(e,"libplaysdk.so"),...):(...)`。

---

## 6. 非 ASCII 路径的 `Public/ZSpace` 复制

**原文**（约 4,889,054 字符处）：

```js
if(/^[\000-\177]*$/.test(ka.dirname(za)))process.emit("vlcPlayerReady",ka.dirname(za)),Qa();else{let t=Oa.replace(/^([a-z]\:\\Users\\).*$/gim,"$1Public");...}
```

**补丁**：

- 锚点（唯一）：`if(/^[\000-\177]*$/.test(ka.dirname(za)))process.emit("vlcPlayerReady",ka.dirname(za)),Qa();else{`
- 替换为：

```js
if("linux"==process.platform||/^[\000-\177]*$/.test(ka.dirname(za)))process.emit("vlcPlayerReady",ka.dirname(za)),Qa();else{
```

**理由**：`else` 分支只在 Windows 的「用户目录含中文」场景使用：
把 `uzplayer` 复制到 `C:\Users\Public\ZSpace\uzplayer`，并把 `za` 改成该副本。
Linux 上没有 `C:\Users\Public`，`Oa.replace(...)` 不匹配会把副本建到
`~/.config/zspace/ZSpace/uzplayer`，再 `za=...UZPlayer.exe` → 路径错误。
强制走 ASCII 分支即可完全跳过复制。

**边界**：

- ASCII 分支会调用 `Qa()`，已被第 2 条改为 no-op（只置 `VlcPreloaded=true`）。
- 该分支还会 `process.emit("vlcPlayerReady", 目录)`，给音乐逻辑用；
  音乐已禁用（第 5 条），无副作用。
- 若用户把应用装在含中文的目录，Linux 也走 ASCII 分支，`za` 仍正确。

---

## 7. 不补丁的部分（保持官方行为）

| 位置 | 原因 |
|---|---|
| `mc(Na)` TCP Server `13600 + rand(20)` | 与播放器无关，保持原样 |
| `Xa()` 帧编码 / `connection` 回调 | 我们的播放器已按此实现，不需要改 |
| `yc()` 中的 `if(!global.VlcPreloaded)return` | 通过第 4 条让条件成立，不改判断 |
| 启动命令的 6 个参数 | 与原版一致，播放器完全兼容 |
| `VlcPreload.exe` 的 toast 文案 | 保持官方文字 |
| `UZPlayer.z`（mac 专用加密 zip） | Linux 永不进入 `if(Ra)` 分支 |
| 外部播放器（第三方播放器设置） | 官方功能；Linux 上的 `taskkill` 未处理，另开议题 |
| `zsviewer` / PDF / Office 预览 | 与播放无关 |

---

## 8. 接入 `patch_app.py` 的建议（主线执行）

新增一个幂等函数，在现有 `patch_bg()` 之后调用：

```python
PATCHES = [
    # (名称, 旧串, 新串)
    ("player-path",
     'za=za.replace("app.asar","app.asar.unpacked");',
     'za=za.replace("app.asar","app.asar.unpacked");"linux"==process.platform&&(za=ka.join(__dirname,"uzplayer/zspace-player"));'),
    ("vlc-preload",
     'Qa=()=>{let e=i.app.getName().toLowerCase()',
     'Qa=()=>{if("linux"==process.platform)return void(global.VlcPreloaded=!0);let e=i.app.getName().toLowerCase()'),
    ("kill-player",
     't=Ra?`killall -9 "${n}"`:`taskkill /f /im "${n}"`',
     't=Ra?`killall -9 "${n}"`:"linux"==process.platform?"pkill -x zspace-player":`taskkill /f /im "${n}"`'),
    ("vlc-preloaded-flag",
     '"darwin"==process.platform?global.VlcPreloaded=!0:Object(Rc.setTimeout)',
     '"darwin"==process.platform||"linux"==process.platform?global.VlcPreloaded=!0:Object(Rc.setTimeout)'),
    ("playsdk-disable",
     ':(Ot=Tt.join(e,"playsdk.dll"),xt=Tt.dirname(Ot),Dt="")});',
     ':"linux"==process.platform?(Ot="",xt="",Dt=""):(Ot=Tt.join(e,"playsdk.dll"),xt=Tt.dirname(Ot),Dt="")});'),
    ("ascii-branch",
     'if(/^[\\000-\\177]*$/.test(ka.dirname(za)))process.emit("vlcPlayerReady",ka.dirname(za)),Qa();else{',
     'if("linux"==process.platform||/^[\\000-\\177]*$/.test(ka.dirname(za)))process.emit("vlcPlayerReady",ka.dirname(za)),Qa();else{'),
]
```

实现要求：

1. 每个补丁先检查「新串」是否已在 `src` 中（幂等），再检查「旧串」是否存在；
   旧串缺失时**只警告、不写入**，最后统一写回并打印每一项结果。
2. 写回前做一次 `node --check background.js`（或 `new Function(src)`）语法校验，
   失败则不落盘、保留原文件。
3. 安装二进制：

```python
dst_dir = os.path.join(APP, "uzplayer")
os.makedirs(dst_dir, exist_ok=True)
dst = os.path.join(dst_dir, "zspace-player")
shutil.copyfile(PLAYER_SRC, dst)
os.chmod(dst, 0o755)
```

4. `PLAYER_SRC` 由 `run-native.sh` 传入（默认
   `native/player/target/release/zspace-player`）；
   若源文件不存在，打印明确错误并退出非 0（不要静默跳过，否则点播放等到 40 s 才报错）。

---

## 9. 验证清单

在 `APP` 目录执行：

```bash
# 1) 语法
node --check background.js

# 2) 6 个标记各出现一次
grep -c 'uzplayer/zspace-player' background.js             # 1
grep -c 'return void(global.VlcPreloaded=!0)' background.js # 1
grep -c 'pkill -x zspace-player' background.js              # 1
grep -c 'Ot="",xt="",Dt=""' background.js                   # 1
grep -c 'linux"==process.platform||/\^\[\\000' background.js # 1（按实际替换串核对）

# 3) 启动应用，看 main.log
#    "VLC video player path:" 结尾是 .../uzplayer/zspace-player
#    没有 "Run VlcPreload error"
#    "Run player by cmd:" 里的第一个参数是 zspace-player
#    点播放后 3 s 内出现 "Player server been connected"
#    关闭时 main.log 有 "stopVideo" 或 "PlayerServer socket ended"

# 4) 播放器侧
ZSPACE_PLAYER_LOG=/tmp/player.log ...   # 启动应用前导出即可
# 期望：已连接主进程 / 收到播放列表 / 开始播放 / 上报 1/2
```

---

## 10. 已知边界汇总

| 场景 | 行为 | 处理 |
|---|---|---|
| 用户目录含中文/空格 | 走 ASCII 分支，不做 Public 复制 | 第 6 条 |
| 播放器二进制缺失 | 连接失败，主进程 40 s 后弹「打开视频时发生错误」 | 安装时校验（第 8 节） |
| 旧播放器残留 | `pkill -x zspace-player` 清理 | 第 3 条 |
| 音乐播放 | 返回 code 1000「音乐播放器未初始化」 | 第 5 条，明确偏离 |
| 第三方播放器设置 | 仍走 `taskkill`，Linux 无效 | 不属于本次 6 处 |
| 远程/转码流 seek、切音轨 | 播放器发 `11/1`/`6/1`，主进程重新出地址 | 播放器侧已实现 |
| 字幕切换 | 播放器发 `11/2`，主进程只回索引 | 播放器侧已实现 |
