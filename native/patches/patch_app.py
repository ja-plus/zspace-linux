#!/usr/bin/env python3
"""Patch the extracted official app for native Linux (M3 + player + sync + mount).

Usage:
    patch_app.py <app-dir> [libzrpc.so] [zspace-player] [syncthing]
                 [zspace-mount-monitor] [rclone]

  1. background.js: load libzrpc.so on linux (was win32 dll fallback).
  2. background.js: 6 player patches from native/player/PATCH-SPEC.md
     (Linux player path, no VlcPreload.exe, pkill, preloaded flag,
     playsdk disabled, skip Public/ZSpace copy).
  3. background.js: sync patches from native/sync/PATCH-SPEC.md
     (bin path -> bin/syncthing, upstream `generate --home=` syntax,
     accept upstream ISO log timestamps).
  4. background.js: mountV2 patches (drop .exe on linux, kill the monitor by
     its 15-char truncated /proc/comm name).
  5. Remove win32 build/Release/*.node so node-gyp-build picks linux prebuilds.
  6. Install our native libzrpc.so into sdk/.
  7. Install zspace-player into uzplayer/ (0755).
  8. Install syncthing into bin/syncthing (0755).
  9. Install zspace-mount-monitor as bin/ZSpaceMountMonitor, rclone as
     bin/ZSpaceMount, and the official filter.txt into bin/filter.txt.
 10. Install zs-fontfix.js next to background.js and inject it into every
     webContents on dom-ready (unify the serif Latin fallback, see FONT_PATCHES).

Idempotent. Validates JS syntax before writing when a JS engine is available
(`ZS_NODE` env, or `node` in PATH; the Electron binary works with
ELECTRON_RUN_AS_NODE=1). Does NOT touch the official exe/asar source.
"""
import glob
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

APP = sys.argv[1] if len(sys.argv) > 1 else '.'
SO_SRC = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] else None
PLAYER_SRC = (
    sys.argv[3]
    if len(sys.argv) > 3 and sys.argv[3]
    else os.path.join(REPO, 'native/player/target/release/zspace-player')
)
SYNC_SRC = (
    sys.argv[4]
    if len(sys.argv) > 4 and sys.argv[4]
    else os.path.join(REPO, 'native/work/syncthing/syncthing')
)
MOUNT_SRC = (
    sys.argv[5]
    if len(sys.argv) > 5 and sys.argv[5]
    else os.path.join(REPO, 'native/mount/target/release/zspace-mount-monitor')
)
RCLONE_SRC = (
    sys.argv[6]
    if len(sys.argv) > 6 and sys.argv[6]
    else os.path.join(REPO, 'native/work/rclone/rclone')
)
FILTER_SRC = os.path.join(REPO, 'packaging-native/mount-filter.txt')
BG = os.path.join(APP, 'background.js')

OLD = '"darwin"==process.platform?"libzrpc.dylib":"libzrpc_x64.dll"'
NEW = '"darwin"==process.platform?"libzrpc.dylib":"linux"==process.platform?"libzrpc.so":"libzrpc_x64.dll"'


def syntax_ok(path):
    """Return True/False, or None when no JS engine is available."""
    node = os.environ.get('ZS_NODE') or shutil.which('node')
    if not node:
        return None
    env = dict(os.environ, ELECTRON_RUN_AS_NODE='1')
    try:
        subprocess.run(
            [node, '--check', path],
            check=True,
            capture_output=True,
            env=env,
        )
        return True
    except subprocess.CalledProcessError as e:
        sys.stderr.write((e.stderr or b'').decode('utf-8', 'replace')[:2000])
        return False


def patch_bg():
    src = open(BG, encoding='utf-8').read()
    if NEW in src:
        print('[bg] linux branch already present')
    elif OLD not in src:
        print('[bg] WARNING: dll-load pattern not found')
    else:
        src = src.replace(OLD, NEW)
        open(BG, 'w', encoding='utf-8').write(src)
        print('[bg] patched dll load -> libzrpc.so on linux')


# (name, old, new) — anchors and replacements from PATCH-SPEC.md
PLAYER_PATCHES = [
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


SYNC_PATCHES = [
    ("sync-bin",
     'this.binPath=U.a.join(__dirname,"bin",nt).replace("app.asar","app.asar.unpacked");',
     'this.binPath=U.a.join(__dirname,"bin",nt).replace("app.asar","app.asar.unpacked");'
     '"linux"==process.platform&&(this.binPath=U.a.join(__dirname,"bin","syncthing"));'),
    ("sync-generate",
     '["-no-browser","-generate="+this.configPath]',
     '"linux"==process.platform?["generate","--home="+this.configPath]:["-no-browser","-generate="+this.configPath]'),
    ("sync-log-date",
     '/^(\\d+\\/\\d+\\/\\d+ \\d+:\\d+:\\d+)/',
     '/^(\\d+[\\/-]\\d+[\\/-]\\d+ \\d+:\\d+:\\d+)/'),
    # The official build kills its Windows binary (ZSpaceSync.exe) with
    # taskkill/killall; our process is `syncthing`, so nothing dies and the
    # next start bumps the API port (8384 -> 28384) while the orphan keeps the
    # old one, leaving the tray popup at "本地服务未启动". Kill by the config
    # path so instances left over from a previous app run are cleaned too.
    ("sync-kill-linux",
     't=Ze?`killall -9 "${nt}"`:`taskkill /f /im "${nt}"`',
     't="linux"==process.platform?`pkill -9 -f "[s]yncthing --no-browser --home=${this.configPath}"`'
     ':Ze?`killall -9 "${nt}"`:`taskkill /f /im "${nt}"`'),
]

# mountV2: monitor/rclone live in <app>/bin. On Linux drop the .exe suffix and
# handle the 15-char /proc/<pid>/comm truncation ("ZSpaceMountMoni").
MOUNT_PATCHES = [
    ("mount-no-exe",
     'vn||(bn+=".exe",wn+=".exe");',
     '"linux"==process.platform||vn||(bn+=".exe",wn+=".exe");'),
    ("mount-kill-comm",
     'e.name.indexOf("MountMonitor")>-1?n=e.pid:e.name.indexOf("Mount")>-1&&(o=e.pid)',
     'e.name.indexOf("MountMonitor")>-1||"linux"==process.platform&&e.name.indexOf("MountMoni")>-1'
     '?n=e.pid:e.name.indexOf("Mount")>-1&&(o=e.pid)'),
]


# Tray: on Linux Electron talks StatusNotifier/AppIndicator, which never emits
# "right-click" and ignores popUpContextMenu() — a menu only shows when it is
# attached with setContextMenu(). Attach the same template the right-click
# handler builds, and rebuild it when the login state flips (menu labels cannot
# be dynamic, they are snapshotted at build time).
#
# NOTE: inside createTray() the identifier `r` is `this.exitApp`, NOT the
# logger (`r.a` lives in other scopes). Logging with r.a here throws
# "Cannot read properties of undefined (reading 'info')", which aborts
# createTray() before any tray.on(...) is registered — that is exactly the
# "tray right-click does nothing" bug. Use console.log instead.
TRAY_PATCHES = [
    ("tray-linux-menu",
     'this.tray.on("right-click",()=>{',
     # 内容必须和 Windows 版一致：登录后是
     #   退出登录 / 隐藏(显示)同步状态 / 分隔线 / 打开极空间 / 退出极空间
     # 同步状态项在 win32 分支里才加，这里显式补上；菜单标签是构建时快照，
     # 所以登录态或 showSyncIcon 变化都要重建（2 s 轮询比较签名）。
     '"linux"==process.platform&&(()=>{let sig="";'
     'const syncOn=()=>{let st=(a.a.state.user.settings&&'
     'a.a.state.user.settings[a.a.state.user.keyName])||{};'
     'return void 0===st.showSyncIcon?!0:!!st.showSyncIcon},'
     'mk=()=>{let e=""!=a.a.state.user.token,sync=syncOn(),t=[...l];'
     'if(e){let st=(a.a.state.user.settings&&'
     'a.a.state.user.settings[a.a.state.user.keyName])||{};'
     't.unshift({type:"separator"});'
     't.unshift({label:sync?Ue.__("yincangton_94856d"):Ue.__("xianshiton_a46a2c"),'
     'click(){st.showSyncIcon=!sync,'
     'a.a.dispatch("user/setPersonalSetting",{key:"showSyncIcon",val:!sync}),u(!sync)}});'
     't.unshift({label:Ue.__("common_button_sign_out"),click(){'
     'let o=a.a.state.user.loginInfo;s(o.remember&&!o.lockNow)}})}'
     'try{this.tray.setContextMenu(i.Menu.buildFromTemplate(t)),sig=e+"/"+sync,'
     'console.log("[tray] linux menu labels="+t.map(x=>x.label||x.type).join("|"))}'
     'catch(x){console.log("[tray] linux menu failed: "+x.message)}};'
     'try{mk(),setInterval(()=>{let e=""!=a.a.state.user.token,sync=syncOn();'
     '(e+"/"+sync)!==sig&&mk()},2e3)}'
     'catch(x){console.log("[tray] linux menu init failed: "+x.message)}})()'
     ',this.tray.on("right-click",()=>{'),
    # 有的 Linux 桌面（KDE/部分 GNOME 扩展）确实会发 right-click；那条路径原来
    # 只给 win32 加「显示/隐藏同步状态」，会和 setContextMenu 挂的菜单不一致。
    # 把 linux 也算进去，两边模板就完全一样了。
    ("tray-rightclick-sync-item",
     'if(e&&"win32"==process.platform){',
     'if(e&&("win32"==process.platform||"linux"==process.platform)){'),
    # Linux: the tray backend does not emit "double-click"; two fast clicks are
    # what the user does. Treat the 2nd click within the DE double-click window
    # as a double-click and show the main window.
    ("tray-linux-doubleclick",
     'this.tray.on("click",e=>{e&&e.preventDefault&&e.preventDefault(),o=!1,',
     'this.tray.on("click",e=>{e&&e.preventDefault&&e.preventDefault(),'
     '"linux"==process.platform&&(()=>{let n=Date.now();'
     'if(n-(this.__lc||0)<400){this.__lc=0,o=!0,'
     'console.log("[tray] double-click -> show app"),c()}else this.__lc=n})(),'
     'o=!1,'),
    # 快捷笔记右键：只要「弹一个菜单」，别再动置顶标记。
    #
    # 排查结论（native.16）：
    # 1) 菜单里出现「检查元素」不是我们这一路弹的 —— 官方把 electron-context-menu
    #    全局挂在了每个窗口上（Pu(...) 里 labels.inspect="检查元素"），笔记窗也在
    #    名单里。两个菜单在同一时刻抢同一个 GTK popup grab：官方的先弹，我们延迟
    #    120 ms 再弹把它顶掉，于是用户看到「闪一下『检查元素』」。
    #    见下面 memo-context-menu：直接在笔记窗上摘掉官方那条监听。
    # 2) 之前为了「菜单被置顶窗口盖住」去 setAlwaysOnTop(!1)，是错的：DDE/X11 上
    #    override-redirect 的 GTK 菜单本来就在置顶窗之上（实测），而改置顶标记会
    #    触发 WM 重新堆叠，堆叠发生在菜单 map 之后 → 菜单刚出来就被撤掉，表现成
    #    「一瞬间就消失」。所以这里不再碰 alwaysOnTop。
    # 3) 仍然显式带 window:Eo + x/y：不带 window 时 Electron 用「当前聚焦窗口」，
    #    笔记窗在非激活态下可能没有焦点，菜单不弹或弹到别的窗口；不带 x/y 会落到
    #    屏幕左上角。
    # 4) 坐标**不要往窗口里夹**（native.18 修）。笔记窗收起时只有 110x48，原来的
    #    `Math.min(x, b.width-180)` 直接把 mx 压成 0，菜单就贴在窗口左下角、看着像
    #    「在光标正下方居中」。GTK 菜单是 override-redirect 的独立窗口，不会被子窗口
    #    裁掉，所以这里按屏幕坐标算：左上角落在光标右下 (+2,+2)，只在贴屏幕边缘时回退。
    # 5) 菜单是 GTK 独立窗口，会拿走焦点；官方页面在失焦/点外面之后会自己发
    #    collapse（收起成图标条）。右键只该弹菜单，不该改窗口状态 → 弹之前记下
    #    So.collapse，关掉后 3 秒内盯一眼，被改了就还原。
    ("memo-rightclick-popup",
     '"rightclick"==e?(No=null,Ao.popup({window:Eo}))',
     '"rightclick"==e?(No=null,(()=>{try{'
     'if(!Eo){r.a.info("[memo] no memo window");return}'
     # context-menu 事件和页面自己发的 "rightclick" 都会走到这里，400 ms 内只认第一次
     'if(Date.now()-(global.__memoMenuAt||0)<400){'
     'r.a.info("[memo] skip duplicate right-click");return}'
     # 记下右键时间：页面自己的 mouseup 会在菜单弹出前后发 collapse/expand，
     # ko() 里据此把这段时间内的折叠/展开丢掉，否则表现成"右键跟左键一样"。
     'global.__memoMenuAt=Date.now();'
     # 让出 120 ms：右键的 mouseup 一般 30~80 ms 就到，太早弹菜单会被 release 立刻
     # 当成「点了第一项」。不涉及置顶/堆叠，纯粹等按键松手。
     'setTimeout(()=>{try{'
     'if(!Eo)return;'
     'let b=Eo.getContentBounds(),p=i.screen.getCursorScreenPoint(),'
     'wa=i.screen.getDisplayNearestPoint(p).workArea,'
     'sx=p.x+2,sy=Math.min(p.y+2,wa.y+wa.height-110),'
     'mx=sx-b.x,my=sy-b.y,t0=Date.now(),cs=!!So.collapse;'
     'global.__memoMenuOpen=!0;'
     'Ao.popup({window:Eo,x:mx,y:my,callback:()=>{'
     'global.__memoMenuOpen=!1,global.__memoMenuAt=Date.now();'
     'r.a.info("[memo] menu closed after "+(Date.now()-t0)+"ms");'
     # 5 s 内每 300 ms 看一眼：折叠态被动了就还原（点菜单项关窗/隐藏时不动）
     '(()=>{let n=0,iv=setInterval(()=>{'
     'if(!Eo||Eo.isDestroyed()||!So.show||++n>17){clearInterval(iv);return}'
     'if(!!So.collapse!==cs){clearInterval(iv),'
     'r.a.info("[memo] revert collapse="+cs+" (page collapsed after menu)"),'
     'cs?Po():Lo()}},300)})()'
     '}}),'
     'r.a.info("[memo] menu popped at "+sx+","+sy+" win="+b.width+"x"+b.height)'
     '}catch(x){r.a.info("[memo] menu popup failed: "+x.message)}},120)'
     '}catch(x){r.a.info("[memo] menu popup failed: "+x.message)}})())'),
    # 页面是 NAS 上的 /home/notebookMemo，右键是否发 "rightclick" 由服务端页面决定；
    # 万一它在这版里不发（或只在 win/darwin 发），主进程就永远收不到，右键就退化成
    # 页面自己的折叠/展开。这里直接挂 webContents 的 context-menu 事件当兜底。
    #
    # 关键：先 removeAllListeners。官方在启动时调了 electron-context-menu（Pu），
    # 它给每个新建窗口的 webContents 都挂了一条 context-menu 监听，弹的是
    # 「复制/粘贴/…/检查元素」的 Chromium 式菜单。笔记窗是透明无边框的悬浮小窗，
    # 这条菜单既没用，又会和我们自己的菜单抢 popup grab —— 用户看到的就是
    # 「闪一下『检查元素』然后消失」。摘掉它，笔记窗只剩我们这一条。
    #
    # 再注入一条 capture 阶段的 stopPropagation：官方页面自己的 contextmenu 处理器
    # 一收到右键就把收起态的小条内容换成「展开/钉住/关闭」三个图标按钮（用户截图里
    # 那个样子）。我们只要弹自己的菜单，不需要页面切这个工具条。
    # 注意：**不能 preventDefault** —— 实测页面 preventDefault 之后，主进程的
    # webContents 'context-menu' 事件就不再触发（我们自己就是靠它兜底的），
    # 只 stopPropagation 则不影响（已用最小页面验证：页面处理器不执行、主进程照常收到）。
    ("memo-context-menu",
     'Eo.on("closed",()=>{Eo=null}),Eo.loadURL(e),Io()}',
     'Eo.on("closed",()=>{Eo=null}),Eo.loadURL(e),'
     'Eo.webContents.removeAllListeners("context-menu"),'
     'Eo.webContents.on("dom-ready",()=>{try{Eo.webContents.executeJavaScript('
     '"(()=>{const s=e=>{2===e.button&&e.stopPropagation()};'
     '[\\"mousedown\\",\\"mouseup\\",\\"pointerdown\\",\\"pointerup\\",\\"contextmenu\\"]'
     '.forEach(t=>window.addEventListener(t,s,!0))})()",!0),'
     'r.a.info("[memo] page right-button events muted")}catch(x){'
     'r.a.info("[memo] mute failed: "+x.message)}}),'
     'Eo.webContents.on("context-menu",()=>{'
     'r.a.info("[memo] webContents context-menu"),ko("rightclick")}),Io()}'),
    # 快捷笔记是「桌面悬浮小窗」，不该在 Dock / 任务栏里占一个位置。
    #
    # 官方代码里其实已经调了 `Eo.setSkipTaskbar(!0)`，但 **Electron 的
    # setSkipTaskbar 只在 Windows/macOS 有实现，Linux 是空操作**。实测（本机
    # DDE/kwin_x11，Electron 33.3.1）：窗口创建后立刻 / ready-to-show / 延迟
    # 1.5 s 三种时机调用，窗口的 `_NET_WM_STATE` 里始终没有
    # `_NET_WM_STATE_SKIP_TASKBAR`（只有 alwaysOnTop 带来的 ABOVE/STAYS_ON_TOP）。
    #
    # 换个路子：dde-shell 的 dock 任务栏（org.deepin.ds.dock.taskmanager）是按
    # `_NET_WM_WINDOW_TYPE` 过滤窗口的 —— 那个 .so 里带着 TOOLBAR / UTILITY /
    # MENU / SPLASH / NOTIFICATION 一串 atom，非 NORMAL 的窗口不进任务栏。
    # 所以把笔记窗的窗口类型改成 `utility`（Electron 在 Linux 支持，落在
    # `_NET_WM_WINDOW_TYPE_UTILITY`）：
    #   · 实测 dock 里图标下的小短横线不再多一条（普通窗口会多一条）；
    #   · 窗口仍能正常拿焦点输入（xdotool windowactivate → focus 事件到位，
    #     document.hasFocus() 为 true），alwaysOnTop / 无边框 / 透明都不受影响；
    #   · Alt-Tab 里也不再列它，符合「悬浮笔记」的语义。
    # 非 Linux 平台传 undefined，等于不传，行为与官方一致。
    ("memo-utility-window",
     'Eo=new i.BrowserWindow({x:t,y:n,width:a,height:c,',
     'Eo=new i.BrowserWindow({x:t,y:n,width:a,height:c,'
     'type:"linux"==process.platform?"utility":void 0,'),
    # 页面的 mouseup 会在右键菜单弹出前后自己发 collapse/expand，
    # 结果"右键 = 左键折叠/展开"。菜单开着时一律丢弃，菜单关掉后 1.2 s 内也丢弃
    # （页面会在失焦后隔一两秒才补发 collapse，800 ms 的窗口挡不住）。
    ("memo-suppress-toggle",
     'ko=e=>(r.a.info("Handle desktop note action:"+e),"hide"==e?',
     'ko=e=>(r.a.info("Handle desktop note action:"+e),'
     '("collapse"==e||"expand"==e)&&'
     '(global.__memoMenuOpen||Date.now()-(global.__memoMenuAt||0)<1200)'
     '?(r.a.info("[memo] drop page toggle after right-click: "+e),!0)'
     ':"hide"==e?'),
    # 收起/展开笔记窗：setPosition 和 setSize 分两步做，会被 WM 按**旧尺寸**夹回屏幕内。
    #
    # 官方 Po() 的顺序是 `setPosition(右对齐 x)` → `setSize(110,48)`。执行 setPosition
    # 时窗口还是展开态的 368 宽：笔记窗默认靠右，x=1238，1238+368=1606 > 1440，
    # Chromium/X11 的 keep-on-screen 把 x 夹成 1072（= 屏幕宽 1440 − 368），随后
    # setSize 缩到 110，位置就停在 1072 —— 用户看到的就是「点开笔记、再关掉，
    # 小条往左挪了一截」。日志里 `update meo pos: {x:1072,...}` 就是这一步。
    # 用 stock Electron 33.3.1 + kwin_x11 单窗口最小验证（368x428 @976,100）：
    #   expand     {x:976,w:368}
    #   old-order  {x:1072,w:110}   ← setPosition 后再 setSize
    #   new-order  {x:1238,w:110}   ← setBounds 一次给尺寸+位置
    # 所以把两步合成一次 setBounds（X11 上走 _NET_MOVERESIZE_WINDOW，WM 按新尺寸算位置）。
    # 展开（Lo）同理：先 setSize 到 368 时窗口还停在靠右的位置，也可能被夹一下再被
    # setPosition 拉回，视觉上闪一帧，一并合成 setBounds。
    ("memo-collapse-bounds",
     'So.collapse=!0,Eo.setMinimumSize(110,48);let e=So.x+So.width-106;'
     'Eo.setPosition(Math.ceil(e),Math.ceil(So.y),!1),'
     'Eo.setSize(Math.ceil(110),Math.ceil(48),!1),'
     'setTimeout(()=>{Eo.setResizable(!1)},20)',
     'So.collapse=!0,Eo.setMinimumSize(110,48);let e=So.x+So.width-106;'
     'Eo.setBounds({x:Math.ceil(e),y:Math.ceil(So.y),width:110,height:48},!1),'
     'setTimeout(()=>{Eo.setResizable(!1)},20)'),
    ("memo-expand-bounds",
     'So.width=Math.max(368,So.width),So.height=Math.max(428,So.height),'
     'Eo.setSize(Math.ceil(So.width),Math.ceil(So.height),!1),'
     'Eo.setPosition(Math.ceil(So.x),Math.ceil(So.y),!1)',
     'So.width=Math.max(368,So.width),So.height=Math.max(428,So.height),'
     'Eo.setBounds({x:Math.ceil(So.x),y:Math.ceil(So.y),'
     'width:Math.ceil(So.width),height:Math.ceil(So.height)},!1)'),
    # 同一个问题还有一处：登录提示弹窗（showPopUpIfNeeded）也用 getBounds()。
    # 一起改成 Linux 用光标锚定。
    ("tray-login-popup-pos",
     'const e=this.tray.getBounds();let t,n=Math.round(e.x+e.width/2-145);',
     'let e=this.tray.getBounds();'
     'if("linux"==process.platform){let p=i.screen.getCursorScreenPoint();'
     'e={x:p.x,y:p.y,width:0,height:0}}'
     'let t,n=Math.round(e.x+e.width/2-145);'),
    # Linux 下 Tray.getBounds() 不可信：有的桌面返回 0,0,0,0，有的返回一个
    # 停在原点的小矩形，官方公式会把它当成图标位置，弹窗就跑到屏幕左上角/别处。
    # 这里在 Linux 上**无条件**用光标位置当锚点（点托盘时鼠标就在图标上），
    # 并把锚点高度设成 32：图标在上半屏时弹窗落在图标下方，下半屏时落在上方。
    ("tray-popup-pos",
     'getTrayPopupPosition(){const e=this.tray.getBounds(),t=i.screen.getAllDisplays(),',
     'getTrayPopupPosition(){let e=this.tray.getBounds();'
     'if("linux"==process.platform){'
     'let p=i.screen.getCursorScreenPoint();'
     'console.log("[tray] popup anchor cursor "+p.x+","+p.y+" bounds="+JSON.stringify(e));'
     'e={x:p.x,y:p.y,width:0,height:32}}'
     'const t=i.screen.getAllDisplays(),'),
]


# 保持自动登录: the official client only keeps the session when
# vuex.json's user.loginInfo.keepLogin is true, but that file is written by the
# renderer's persisted store and never gets the post-login mutation on Linux
# (mtime stays at the last login-page session), so every start cleared the token
# and went back to the login page. Persist/replay the sign-in payload ourselves.
KEEP_LOGIN_PATCHES = [
    ("keep-login-restore",
     't.keepLogin||a.a.dispatch("user/setToken","")',
     '(()=>{try{Object(V.readFile)(i.app.getPath("userData")+"/keep-login.json","utf8",(x,d)=>{'
     'if(x)return console.log("[keep-login] none ("+x.code+")");'
     'try{let f=JSON.parse(d);if(!f||!f.user||!f.user.token)return;'
     'a.a.dispatch("user/setUserName",f.user.username),'
     'a.a.dispatch("user/setQcName",f.user.qcname),'
     'a.a.dispatch("user/setMaster",f.user.isMaster),'
     'a.a.dispatch("user/setToken",f.user.token),'
     'a.a.dispatch("user/setAvatar",f.user.avatar),'
     'a.a.dispatch("nas/setNasIp",f.nas.nasIp),'
     'a.a.dispatch("nas/setDevColor",f.nas.color),'
     'a.a.dispatch("nas/setDevModel",f.nas.deviceMode),'
     'a.a.dispatch("nas/setNasId",f.nas.nasId),'
     'a.a.dispatch("nas/setNasName",f.nas.nasName),'
     'f.nas.nasPubKey&&a.a.dispatch("nas/setPubKey",{nasPubKey:f.nas.nasPubKey,sign:f.nas.sign}),'
     'a.a.dispatch("user/setLoginInfo",{keepLogin:!0,remember:!0}),'
     't.keepLogin=!0,'
     'console.log("[keep-login] restored "+f.user.username)}catch(y){'
     'console.log("[keep-login] parse failed: "+y.message)}})}catch(x){'
     'console.log("[keep-login] read error: "+x.message)}})(),'
     't.keepLogin||a.a.dispatch("user/setToken","")'),
    ("keep-login-save",
     'r.a.info("setStoreFromClient time:"+((new Date).getTime()-o)),n?(',
     'r.a.info("setStoreFromClient time:"+((new Date).getTime()-o)),'
     '(()=>{try{Uu.writeFile(i.app.getPath("userData")+"/keep-login.json",'
     'JSON.stringify(t),{mode:384},x=>{x?console.log("[keep-login] save failed: "+x.message):'
     'console.log("[keep-login] saved session for "+t.user.username)})}catch(x){'
     'console.log("[keep-login] save error: "+x.message)}})(),n?('),
    ("keep-login-clear",
     'a.a.dispatch("user/setToken",""),a.a.dispatch("user/setUserName","")',
     'a.a.dispatch("user/setToken",""),(()=>{try{'
     'Uu.unlink(i.app.getPath("userData")+"/keep-login.json",()=>{})}catch(x){}})(),'
     'a.a.dispatch("user/setUserName","")'),
]



# 旧补丁文本 → 新补丁文本。build 时会拿 native/work/app/extracted 这种
# "已经打过上一版补丁"的目录直接用，所以必须能把旧文本升级掉，否则
# 幂等判断（new in src）不成立，anchor 又会命中，导致同一段被插两次。
LEGACY = {
    'tray-linux-menu': [(
        '"linux"==process.platform&&(()=>{let last=null,mk=()=>{'
        'let e=""!=a.a.state.user.token,t=[...l];'
        'e&&t.push({label:Ue.__("common_button_sign_out"),click(){'
        'let i=a.a.state.user.loginInfo;s(i.remember&&!i.lockNow)}});'
        'try{this.tray.setContextMenu(i.Menu.buildFromTemplate(t)),last=e,'
        'console.log("[tray] linux menu set login="+e)}catch(x){'
        'console.log("[tray] linux menu failed: "+x.message)}};'
        'try{mk(),setInterval(()=>{(""!=a.a.state.user.token)!==last&&mk()},2e3)}'
        'catch(x){console.log("[tray] linux menu init failed: "+x.message)}})()'
        # 旧补丁文本末尾带着 anchor（新文本也带），升级时必须一起替换，
        # 否则 anchor 会残留一次，变成 `)(),this.tray.on("right-click",()=>{,this.tray.on(...`
        ',this.tray.on("right-click",()=>{',
        'TRAY_NEW',
    )],
    'memo-rightclick-popup': [(
        # native.20：横向还留着 200 px 固定边距，贴屏幕右侧时会把菜单往左拽
        '"rightclick"==e?(No=null,(()=>{try{if(!Eo){r.a.info("[memo] no memo window");return}if(Date.now()-(global.__memoMenuAt||0)<400){r.a.info("[memo] skip duplicate right-click");return}global.__memoMenuAt=Date.now();setTimeout(()=>{try{if(!Eo)return;let b=Eo.getContentBounds(),p=i.screen.getCursorScreenPoint(),wa=i.screen.getDisplayNearestPoint(p).workArea,sx=Math.min(p.x+2,wa.x+wa.width-200),sy=Math.min(p.y+2,wa.y+wa.height-150),mx=sx-b.x,my=sy-b.y,t0=Date.now(),cs=!!So.collapse;global.__memoMenuOpen=!0;Ao.popup({window:Eo,x:mx,y:my,callback:()=>{global.__memoMenuOpen=!1,global.__memoMenuAt=Date.now();r.a.info("[memo] menu closed after "+(Date.now()-t0)+"ms");(()=>{let n=0,iv=setInterval(()=>{if(!Eo||Eo.isDestroyed()||!So.show||++n>17){clearInterval(iv);return}if(!!So.collapse!==cs){clearInterval(iv),r.a.info("[memo] revert collapse="+cs+" (page collapsed after menu)"),cs?Po():Lo()}},300)})()}}),r.a.info("[memo] menu popped at "+sx+","+sy+" win="+b.width+"x"+b.height)}catch(x){r.a.info("[memo] menu popup failed: "+x.message)}},120)}catch(x){r.a.info("[memo] menu popup failed: "+x.message)}})())',
        'MEMO_NEW',
    ), (
        # native.17 线上文本：坐标往窗口里夹（小窗时菜单贴到左下角）+ 不还原折叠态
        '"rightclick"==e?(No=null,(()=>{try{if(!Eo){r.a.info("[memo] no memo window");return}'
        'if(Date.now()-(global.__memoMenuAt||0)<400){r.a.info("[memo] skip duplicate right-click");return}'
        'global.__memoMenuAt=Date.now();setTimeout(()=>{try{if(!Eo)return;'
        'let b=Eo.getContentBounds(),p=i.screen.getCursorScreenPoint(),'
        'mx=Math.max(0,Math.min(p.x-b.x,b.width-180)),'
        'my=Math.max(0,Math.min(p.y-b.y,b.height-16)),t0=Date.now();'
        'Ao.popup({window:Eo,x:mx,y:my,callback:()=>{'
        'r.a.info("[memo] menu closed after "+(Date.now()-t0)+"ms")}}),'
        'r.a.info("[memo] menu popped at "+mx+","+my+" win="+b.width+"x"+b.height)'
        '}catch(x){r.a.info("[memo] menu popup failed: "+x.message)}},120)'
        '}catch(x){r.a.info("[memo] menu popup failed: "+x.message)}})())',
        'MEMO_NEW',
    ), (
        # native.14 / native.15 线上文本：popup 期间关置顶 + 用 console.log
        '"rightclick"==e?(No=null,(()=>{try{if(!Eo){console.log("[memo] no memo window");return}'
        'if(Date.now()-(global.__memoMenuAt||0)<400){console.log("[memo] skip duplicate right-click");return}'
        'global.__memoMenuAt=Date.now();let w=Eo.isAlwaysOnTop();Eo.setAlwaysOnTop(!1);'
        'setTimeout(()=>{try{if(!Eo)return;let b=Eo.getContentBounds(),'
        'p=i.screen.getCursorScreenPoint(),'
        'mx=Math.max(0,Math.min(p.x-b.x,b.width-180)),'
        'my=Math.max(0,Math.min(p.y-b.y,b.height-16));'
        'Ao.popup({window:Eo,x:mx,y:my,callback:()=>{Eo&&w&&Eo.setAlwaysOnTop(!0)}}),'
        'console.log("[memo] menu popped at "+mx+","+my+" wasOnTop="+w)}catch(x){'
        'console.log("[memo] menu popup failed: "+x.message),Eo&&w&&Eo.setAlwaysOnTop(!0)}},120)'
        '}catch(x){console.log("[memo] menu popup failed: "+x.message),'
        'Eo&&Eo.setAlwaysOnTop(!0)}})())',
        'MEMO_NEW',
    ), (
        # native.11 的中间版本：同步 popup + 关置顶
        '"rightclick"==e?(No=null,(()=>{try{let w=Eo&&Eo.isAlwaysOnTop();'
        'Eo&&Eo.setAlwaysOnTop(!1),'
        'Ao.popup({callback:()=>{Eo&&w&&Eo.setAlwaysOnTop(!0)}}),'
        'console.log("[memo] menu popped, wasOnTop="+w)}catch(x){'
        'console.log("[memo] menu popup failed: "+x.message),'
        'Eo&&Eo.setAlwaysOnTop(!0)}})())',
        'MEMO_NEW',
    ), (
        # native.12 的中间版本：window:Eo + x/y 但同步 popup、不吞折叠
        '"rightclick"==e?(No=null,(()=>{try{'
        'if(!Eo){console.log("[memo] no memo window");return}'
        'let w=Eo.isAlwaysOnTop(),b=Eo.getContentBounds(),'
        'p=i.screen.getCursorScreenPoint(),'
        'mx=Math.max(0,Math.min(p.x-b.x,b.width-180)),'
        'my=Math.max(0,Math.min(p.y-b.y,b.height-16));'
        'Eo.setAlwaysOnTop(!1),'
        'Ao.popup({window:Eo,x:mx,y:my,callback:()=>{Eo&&w&&Eo.setAlwaysOnTop(!0)}}),'
        'console.log("[memo] menu popped at "+mx+","+my+" wasOnTop="+w)}catch(x){'
        'console.log("[memo] menu popup failed: "+x.message),'
        'Eo&&Eo.setAlwaysOnTop(!0)}})())',
        'MEMO_NEW',
    ), (
        # native.13 版本：延迟 120ms + 吞折叠，但还没有"重复右键去抖"
        '"rightclick"==e?(No=null,(()=>{try{if(!Eo){console.log("[memo] no memo window");return}global.__memoMenuAt=Date.now();let w=Eo.isAlwaysOnTop();Eo.setAlwaysOnTop(!1);setTimeout(()=>{try{if(!Eo)return;let b=Eo.getContentBounds(),p=i.screen.getCursorScreenPoint(),mx=Math.max(0,Math.min(p.x-b.x,b.width-180)),my=Math.max(0,Math.min(p.y-b.y,b.height-16));Ao.popup({window:Eo,x:mx,y:my,callback:()=>{Eo&&w&&Eo.setAlwaysOnTop(!0)}}),console.log("[memo] menu popped at "+mx+","+my+" wasOnTop="+w)}catch(x){console.log("[memo] menu popup failed: "+x.message),Eo&&w&&Eo.setAlwaysOnTop(!0)}},120)}catch(x){console.log("[memo] menu popup failed: "+x.message),Eo&&Eo.setAlwaysOnTop(!0)}})())',
        'MEMO_NEW',
    )],
    'memo-context-menu': [(
        # native.19：只屏蔽了 contextmenu，页面其实是靠 mousedown(button=2) 切工具条
        'Eo.on("closed",()=>{Eo=null}),Eo.loadURL(e),Eo.webContents.removeAllListeners("context-menu"),Eo.webContents.on("dom-ready",()=>{try{Eo.webContents.executeJavaScript("window.addEventListener(\\"contextmenu\\",e=>e.stopPropagation(),!0)",!0),r.a.info("[memo] page contextmenu handler muted")}catch(x){r.a.info("[memo] mute failed: "+x.message)}}),Eo.webContents.on("context-menu",()=>{r.a.info("[memo] webContents context-menu"),ko("rightclick")}),Io()}',
        'CTX_NEW',
    ), (
        # native.18：摘掉了官方菜单，但没屏蔽页面自己的 contextmenu 处理器
        'Eo.on("closed",()=>{Eo=null}),Eo.loadURL(e),'
        'Eo.webContents.removeAllListeners("context-menu"),'
        'Eo.webContents.on("context-menu",()=>{'
        'r.a.info("[memo] webContents context-menu"),ko("rightclick")}),Io()}',
        'CTX_NEW',
    ), (
        # native.14 / native.15：只挂了自己的兜底监听，没摘掉 electron-context-menu
        'Eo.on("closed",()=>{Eo=null}),Eo.loadURL(e),'
        'Eo.webContents.on("context-menu",()=>{'
        'console.log("[memo] webContents context-menu"),ko("rightclick")}),Io()}',
        'CTX_NEW',
    )],
    'memo-suppress-toggle': [(
        # native.14~17：800 ms 窗口 + console.log，且不认「菜单开着」这个状态
        'ko=e=>(r.a.info("Handle desktop note action:"+e),'
        '("collapse"==e||"expand"==e)&&Date.now()-(global.__memoMenuAt||0)<800'
        '?(console.log("[memo] drop page toggle after right-click: "+e),!0)'
        ':"hide"==e?',
        'SUP_NEW',
    )],
    'tray-popup-pos': [(
        'getTrayPopupPosition(){let e=this.tray.getBounds();'
        '"linux"==process.platform&&(!e||0==e.width&&0==e.height)&&(()=>{'
        'let p=i.screen.getCursorScreenPoint();'
        'e={x:p.x,y:p.y,width:0,height:0},'
        'console.log("[tray] popup anchored at cursor "+p.x+","+p.y)})();'
        'const t=i.screen.getAllDisplays(),',
        'POPUP_NEW',
    )],
}

# 字体统一：NAS 页面把根节点字体栈写成了全角逗号
#   <div id="app" style='font-family: "Microsoft YaHei，Arial，Verdana，sans-serif";'>
# 整串被 Chromium 当成一个字体名 → 西文落到 UA 默认的 Times New Roman（衬线），
# 中文落到 Source Han Sans SC（黑体），同一个界面里中英文两种字型。
# Windows/Wine 的 UA 默认字体恰好是无衬线的，所以官方没暴露这个问题。
#
# 修法：给每个 webContents 在 dom-ready 阶段注入 zs-fontfix.js —— 该脚本给
# <html> 打统一字体栈，并把带全角逗号的 inline font-family 就地改写。
# 只动 font-family，图标字体的元素不含全角逗号，不会被命中。
FONT_PATCHES = [
    ("font-unify",
     'i.app.on("web-contents-created",(e,t)=>{t.setWindowOpenHandler(({url:e})=>{',
     'i.app.on("web-contents-created",(e,t)=>{'
     '"linux"==process.platform&&(()=>{let _zf;'
     'try{_zf=require("fs").readFileSync(require("path").join(__dirname,"zs-fontfix.js"),"utf8")}'
     'catch(x){return void console.log("[font] fixer unreadable: "+x.message)}'
     't.on("dom-ready",()=>{t.executeJavaScript(_zf,!0)'
     '.catch(x=>console.log("[font] inject failed: "+x.message))})})(),'
     't.setWindowOpenHandler(({url:e})=>{'),
]


def apply_patches(tag, patches):
    src = open(BG, encoding='utf-8').read()
    changed = False
    results = []
    for name, old, new in patches:
        for lo, ln in LEGACY.get(name, []):
            if ln in ('TRAY_NEW', 'MEMO_NEW', 'POPUP_NEW', 'CTX_NEW', 'SUP_NEW'):
                ln = new
            if lo in src and lo != ln:
                src = src.replace(lo, ln, 1)
                changed = True
                results.append((name, 'upgraded from legacy'))
        if new in src:
            results.append((name, 'already'))
            continue
        count = src.count(old)
        if count == 0:
            results.append((name, 'WARNING: anchor not found (skipped)'))
            continue
        if count > 1:
            results.append((name, f'WARNING: anchor x{count} (skipped)'))
            continue
        src = src.replace(old, new, 1)
        changed = True
        results.append((name, 'patched'))

    if changed:
        # Keep the .js suffix: Node/Electron refuse unknown extensions in
        # `--check` mode.
        fd, tmp = tempfile.mkstemp(dir=APP, prefix='.bgcheck-', suffix='.js')
        os.close(fd)
        try:
            with open(tmp, 'w', encoding='utf-8') as f:
                f.write(src)
            ok = syntax_ok(tmp)
            if ok is False:
                print(f'[{tag}] ERROR: patched JS failed syntax check; not written')
                os.remove(tmp)
                sys.exit(2)
            if ok is None:
                print(f'[{tag}] note: no JS engine for syntax check (ZS_NODE/node)')
            os.replace(tmp, BG)
        finally:
            if os.path.exists(tmp):
                os.remove(tmp)

    for name, state in results:
        print(f'[{tag}] {name}: {state}')
    if any(s.startswith('WARNING') for _, s in results):
        print(f'[{tag}] some anchors were missing; feature may not work (check PATCH-SPEC.md)')


def strip_win32_bindings():
    n = 0
    for f in glob.glob(os.path.join(APP, 'node_modules', '**', 'build', 'Release', '*.node'), recursive=True):
        with open(f, 'rb') as fh:
            magic = fh.read(2)
        if magic == b'MZ':
            os.remove(f)
            n += 1
    print(f'[bindings] removed {n} win32 .node (linux prebuilds remain)')


def install_fontfix():
    """渲染进程字体修正脚本，放在 background.js 旁边（__dirname 即 app 根）。"""
    src = os.path.join(HERE, 'fontfix.js')
    if not os.path.isfile(src):
        print(f'[font] ERROR: {src} missing')
        sys.exit(3)
    dst = os.path.join(APP, 'zs-fontfix.js')
    if os.path.abspath(src) != os.path.abspath(dst):
        shutil.copyfile(src, dst)
    print(f'[font] installed {dst} ({os.path.getsize(dst)} bytes)')


def install_so():
    if not SO_SRC:
        print('[so] no source given, skip')
        return
    dst_dir = os.path.join(APP, 'sdk')
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, 'libzrpc.so')
    if os.path.abspath(SO_SRC) != os.path.abspath(dst):
        shutil.copyfile(SO_SRC, dst)
    print(f'[so] installed {dst} ({os.path.getsize(dst)} bytes)')


def install_player():
    if not PLAYER_SRC or not os.path.isfile(PLAYER_SRC):
        print(f'[player] ERROR: player binary not found: {PLAYER_SRC}')
        print('[player] build it first: cd native/player && cargo build --release')
        sys.exit(3)
    dst_dir = os.path.join(APP, 'uzplayer')
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, 'zspace-player')
    if os.path.abspath(PLAYER_SRC) != os.path.abspath(dst):
        shutil.copyfile(PLAYER_SRC, dst)
    os.chmod(dst, 0o755)
    print(f'[player] installed {dst} ({os.path.getsize(dst)} bytes)')


def install_sync():
    if not SYNC_SRC or not os.path.isfile(SYNC_SRC):
        print(f'[sync] WARNING: syncthing binary not found: {SYNC_SRC}')
        print('[sync] download 1.27.7 into native/work/syncthing/syncthing or pass arg 4')
        return
    dst_dir = os.path.join(APP, 'bin')
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, 'syncthing')
    if os.path.abspath(SYNC_SRC) != os.path.abspath(dst):
        shutil.copyfile(SYNC_SRC, dst)
    os.chmod(dst, 0o755)
    print(f'[sync] installed {dst} ({os.path.getsize(dst)} bytes)')


def install_mount():
    bin_dir = os.path.join(APP, 'bin')
    os.makedirs(bin_dir, exist_ok=True)
    pairs = [
        ('ZSpaceMountMonitor', MOUNT_SRC, 0o755),
        ('ZSpaceMount', RCLONE_SRC, 0o755),
        ('filter.txt', FILTER_SRC, 0o644),
    ]
    for name, src, mode in pairs:
        if not src or not os.path.isfile(src):
            print(f'[mount] WARNING: missing {name} source: {src}')
            continue
        dst = os.path.join(bin_dir, name)
        if os.path.abspath(src) != os.path.abspath(dst):
            shutil.copyfile(src, dst)
        os.chmod(dst, mode)
        print(f'[mount] installed {dst} ({os.path.getsize(dst)} bytes)')


if __name__ == '__main__':
    patch_bg()
    apply_patches('player', PLAYER_PATCHES)
    apply_patches('sync', SYNC_PATCHES)
    apply_patches('mount', MOUNT_PATCHES)
    apply_patches('tray', TRAY_PATCHES)
    apply_patches('keeplogin', KEEP_LOGIN_PATCHES)
    apply_patches('font', FONT_PATCHES)
    strip_win32_bindings()
    install_fontfix()
    install_so()
    install_player()
    install_sync()
    install_mount()
    print('done')
