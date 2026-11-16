# sources.md —— 官方来源与接口

> 给 Agent 查事实用。改这里的前提是真的去官网核对过。

## 1. 官方下载页

- 下载站：<https://download.zspace.cn/>
- 站点 JS 里的包清单定义了每个平台的 `app_id` 与 `channel`。

## 2. 版本查询接口（本项目用的）

```
GET https://upgrade.zenithspace.net/upgrade_server/v2/check_upgrade
    ?app_id=APP_ZSPACE_DESKTOP_WIN
    &app_version=V0.0.0
    &nas_id=
    &plat=app
    &channel=zspace
    &skip_app_sync_upgrade=1
```

来源：下载站 `https://download.zspace.cn/` 页面内联脚本里的 `fetchVersionInfo()`。
注意域名是 **upgrade**.zenithspace.net，不是 `update.zenithspace.net`（那是静态文件桶），
也不是 `api.zenithspace.net`（那是业务接口）。

### 响应字段

| 字段 | 含义 |
|---|---|
| `code` | `"200"` 为成功 |
| `data.app_version` | 最新版本号，形如 `V2.46.2026101501` |
| `data.download_url` | 安装包直链（`update.zenithspace.net/.../*.exe`） |
| `data.package_size` | 字节数，可用来校验下载完整性 |
| `data.update_time` | 发布时间，`YYYY-MM-DD HH:MM:SS` |
| `data.version_content` | 更新日志（`\r\n` 分行） |
| `data.publish_id` | 发布序号，只作记录 |

### 其它 app_id / channel（备用）

| 平台 | app_id | channel |
|---|---|---|
| Windows 客户端 | `APP_ZSPACE_DESKTOP_WIN` | `zspace` |
| Win7 兼容版 | `APP_ZSPACE_DESKTOP_WIN7` | `zspace` |
| macOS (Intel) | `APP_ZSPACE_DESKTOP_MAC` | `zspace` |
| macOS (Apple Silicon) | `APP_ZSPACE_DESKTOP_MACARM` | `zspace` |
| macOS 10 兼容版 | `APP_ZSPACE_DESKTOP_MAC10` | `zspace` |
| Android | `APP_ZSPACE_ANDROID` | `zspace` |
| Android 亲情版 | `APP_ZSPACE_ANDROID_FAMILY` | `fm_zspace` |
| TV 正式版 | `APP_ZSPACE_ANDROID_TV` | `tm` / `zspace` |
| TV 尝鲜版 | `APP_ZSPACE_ANDROID_TV` | `beta` |
| 车机版 | `APP_ZSPACE_ANDROID_AUTO` | `zspace` |

## 3. 安装包命名

```
zspace_windows_<版本>_<时间戳>.exe
例：zspace_windows_2.46.2026101501_101619.exe
```

`scripts/build_deb.sh` 靠这个文件名解析版本号，**下载后不要改名**。

## 4. 安装包内部结构（实测，V2.46.2026101501）

```
zspace_windows_*.exe            NSIS 安装包（约 321 MB）
└── $PLUGINSDIR/app-64.7z       electron-builder 产物（约 320 MB）
    └── (解出) app/
        ├── 极空间.exe          Electron 主程序（约 189 MB）
        ├── resources/app.asar  业务代码（约 26 MB，未加固，可直接读）
        ├── resources/app.asar.unpacked/
        └── icudtl.dat / *.pak / *.dll …   Electron 33.3.1 运行时
```

实测要点：

- Electron 33.3.1 / Chromium 130 / Node 20.18.1。
- `app.asar` **未加固**，`background.js` 可直接 grep，便于排查。
- 核心闭源库 `sdk/libzrpc_x64.dll` 只有 Win/mac 版本。
- 客户端原生支持 IP 直连（端口 5055），可绕开 libzrpc。

## 5. 当前已知版本

| 版本 | 发布时间 | 本地是否有包 |
|---|---|---|
| V2.46.2026101501 | 2026-10-21 11:13:11 | 有（packages/） |

> 有新版本时**追加一行**，不要改历史记录。

## 6. 同步运行时（syncthing）

- 官方 `resources/app.asar.unpacked/bin/ZSpaceSync.exe` 是 syncthing
  **v1.27.7** 的 fork（`bin/version.txt` = `v1.27.7-xnas-c20260624135721`）。
- 本项目使用上游同版本 Linux amd64：
  `https://github.com/syncthing/syncthing/releases/download/v1.27.7/syncthing-linux-amd64-v1.27.7.tar.gz`
- SHA256：`068f3cf972113d101666678dd7b043d9363356667b8c4a81c84b1b54d4d342c3`
- 下载脚本：`scripts/fetch_syncthing.sh`；落盘 `native/work/syncthing/syncthing`
  （`native/work/` 已 gitignore，不入库）。
- 不要用 2.x：配置/数据库会迁移，与官方 fork 不兼容。

## 7. 挂载运行时（rclone）

- 官方 `bin/ZSpaceMount.exe` 是 rclone **v1.70.2** 的 fork
  （`v1.70.2-20250702150924-windows-amd64`，tags cmount）。
- 本项目使用上游同版本 Linux amd64：
  `https://downloads.rclone.org/v1.70.2/rclone-v1.70.2-linux-amd64.zip`
- zip SHA256：`7bfa813f426820d20436e48a4e0b9bf21977fcd513a48f2d28eede3136b4bacb`
- 二进制 SHA256：`d1d99f58c1ad31592caa093eb400607725afd74eb3655bdeb6dc1886fed8fda8`
- 下载脚本：`scripts/fetch_rclone.sh`；落盘 `native/work/rclone/rclone`
  （`native/work/` 已 gitignore，不入库）。
- 官方 `bin/filter.txt`（50 条排除规则）随包：
  `packaging-native/mount-filter.txt`。


