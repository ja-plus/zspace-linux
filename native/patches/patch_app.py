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
    # 快捷笔记: the page sends "rightclick" on mousedown; main pops a native menu.
    # Two Linux problems: (1) the memo window is alwaysOnTop, so the menu is
    # drawn *behind* it and never visible — drop the flag while the menu is
    # open and restore it in the popup callback; (2) do NOT defer the popup:
    # deferring lets the page's mouseup run its "expand" toggle, which is the
    # reported "right-click behaves like left-click".
    ("memo-rightclick-popup",
     '"rightclick"==e?(No=null,Ao.popup({window:Eo}))',
     # 三个 Linux 问题一起处理：
     # 1) 悬浮笔记窗口始终置顶，菜单会被它盖住 → popup 期间临时取消置顶；
     # 2) popup 必须显式带上 window:Eo —— 只给 callback 时 Electron 用"当前聚焦
     #    窗口"，而笔记窗在小窗/非激活状态下可能没有焦点，菜单就不弹或弹错窗口；
     # 3) 给 x/y（窗口内容坐标，夹在窗口范围内）让菜单贴着鼠标出现，
     #    否则它会落到屏幕左上角。
     '"rightclick"==e?(No=null,(()=>{try{'
     'if(!Eo){console.log("[memo] no memo window");return}'
     # context-menu 事件和页面自己发的 "rightclick" 都会走到这里，400 ms 内只认第一次
     'if(Date.now()-(global.__memoMenuAt||0)<400){'
     'console.log("[memo] skip duplicate right-click");return}'
     # 记下右键时间：页面自己的 mouseup 会在菜单弹出前后发 collapse/expand，
     # ko() 里据此把这段时间内的折叠/展开丢掉，否则表现成"右键跟左键一样"。
     'global.__memoMenuAt=Date.now();'
     'let w=Eo.isAlwaysOnTop();'
     'Eo.setAlwaysOnTop(!1);'
     # 置顶标记是 WM 属性，X11/XWayland 下都不是同步生效；立刻 popup 会在
     # 标记生效前就 map 出来，于是菜单仍被笔记窗盖住。让出 120 ms 再弹。
     'setTimeout(()=>{try{'
     'if(!Eo)return;'
     'let b=Eo.getContentBounds(),p=i.screen.getCursorScreenPoint(),'
     'mx=Math.max(0,Math.min(p.x-b.x,b.width-180)),'
     'my=Math.max(0,Math.min(p.y-b.y,b.height-16));'
     'Ao.popup({window:Eo,x:mx,y:my,callback:()=>{Eo&&w&&Eo.setAlwaysOnTop(!0)}}),'
     'console.log("[memo] menu popped at "+mx+","+my+" wasOnTop="+w)'
     '}catch(x){console.log("[memo] menu popup failed: "+x.message),'
     'Eo&&w&&Eo.setAlwaysOnTop(!0)}},120)'
     '}catch(x){console.log("[memo] menu popup failed: "+x.message),'
     'Eo&&Eo.setAlwaysOnTop(!0)}})())'),
    # 页面是 NAS 上的 /home/notebookMemo，右键是否发 "rightclick" 由服务端页面决定；
    # 万一它在这版里不发（或只在 win/darwin 发），主进程就永远收不到，右键就退化成
    # 页面自己的折叠/展开。这里直接挂 webContents 的 context-menu 事件当兜底：
    # 只要真的右键了就会触发，和页面实现无关。
    ("memo-context-menu",
     'Eo.on("closed",()=>{Eo=null}),Eo.loadURL(e),Io()}',
     'Eo.on("closed",()=>{Eo=null}),Eo.loadURL(e),'
     'Eo.webContents.on("context-menu",()=>{'
     'console.log("[memo] webContents context-menu"),ko("rightclick")}),Io()}'),
    # 页面的 mouseup 会在右键菜单弹出前后自己发 collapse/expand，
    # 结果"右键 = 左键折叠/展开"。右键后 800 ms 内的折叠/展开直接丢弃。
    ("memo-suppress-toggle",
     'ko=e=>(r.a.info("Handle desktop note action:"+e),"hide"==e?',
     'ko=e=>(r.a.info("Handle desktop note action:"+e),'
     '("collapse"==e||"expand"==e)&&Date.now()-(global.__memoMenuAt||0)<800'
     '?(console.log("[memo] drop page toggle after right-click: "+e),!0)'
     ':"hide"==e?'),
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

def apply_patches(tag, patches):
    src = open(BG, encoding='utf-8').read()
    changed = False
    results = []
    for name, old, new in patches:
        for lo, ln in LEGACY.get(name, []):
            if ln in ('TRAY_NEW', 'MEMO_NEW', 'POPUP_NEW'):
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
    strip_win32_bindings()
    install_so()
    install_player()
    install_sync()
    install_mount()
    print('done')
