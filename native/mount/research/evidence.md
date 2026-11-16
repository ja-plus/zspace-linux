# 挂载线证据（官方行为）

取证对象：本机已安装的官方 Windows 客户端 V2.46.2026101501
（`~/.deepinwine/com.zspace.spark/drive_c/Program Files/zspace/resources/app.asar.unpacked/`）。
全部为静态读取 + Wine 隔离 prefix 下运行官方 CLI；未登录账号、未访问真实 NAS。

---

## 1. background.js：mountV2 是当前生效路径

```js
// 全局固定置真
global.supportMountV2 = !0

// mountS3IfNeeded（旧路径）
let o = global.webClientVersion.split(".").pop();
global.supportMountV2 && o >= "2025070101" ||
  b.getAllServices().then(...)   // 旧路径兜底

// mountV2 自动挂载
let n = global.webClientVersion.split(".").pop();
!global.supportMountV2 || n < "2025070101" ||
  b.getAllServices().then(...).then(() => po(!0))
```

当前版本 `2.46.2026101501` → `webClientVersion` 尾段 `2026101501 >= 2025070101`。

### 1.1 home-dir 与文件

```js
if ("zspace" == yn && /^\d.+/.test(Dn)) e = Dn; else e += "_" + Pn;
Ln = _n.join(i.app.getPath("userData"), e, "mount");
// filter.txt
xn = _n.join(Ln, "filter.txt");
if (!gn.existsSync(xn)) gn.copyFileSync(_n.join(In, "filter.txt"), xn);
```

### 1.2 写 conf

```js
const t = ["config", "--add", "--program=" + Cn, "--home-dir=" + Ln,
  "--url=http://127.0.0.1:" + Kn, "--user=" + e, "--pass=" + Un, "--obscure"];
```

`Kn` 来自：

```js
encodeService({ local_port: 13600, remote_port: 9001, type: 1,
                nas_id: nasId, request_purpose: 8 })
```

### 1.3 挂载

```js
const n = ["--program=" + Cn, "--home-dir=" + Ln, "--cache-dir=" + zn,
  "--cache-max-size=10G", "--volume-label=" + Sn];
// mac:
n.push("--volume=" + yn); n.push("--host-name=" + Tn + ".local");
// win:
n.push("--volume=" + jn);
if (e || vn) n.push("--show-hidden");
if ("vv" == ro("logLevel")) n.push("--rc-o=-vv");
no(kn, n, !0, 0);
```

### 1.4 状态 / 卸载

```js
// mount.port
const t = gn.readFileSync(_n.join(Ln, "mount.port"), "utf8");
Jn = parseInt(t.trim(), 10);

// 状态
["rc", "--rc-addr=http://127.0.0.1:" + Jn + "/mount/status"]
Bn = t.success && t.data.is_running && t.data.is_mounting

// 卸载
["rc", "--rc-addr=http://127.0.0.1:" + Jn + "/shutdown"]
// 然后 process --list --name=ZSpaceMount* → process --kill --pid=<pid>
// mac 额外：umount -f <cachePath>/极空间-个人
```

### 1.5 旧路径（未生效，仅留档）

```js
Ht = new Ft.Library(t, {
  MountInit: ["int", ["string"]],
  UnMountBucket: ["bool", ["string"]],
  MountBucketEX: ["string", ["string","string","string","string","string",
                             "string","string","string","bool","string"]],
  UnMountAll: ["void", []]
});
Ht.MountBucketEX(diskCode, username, s3key, port, cacheDir, "zspace",
                 mountPath, "极空间-个人", showHidden, "10G");
```

旧路径写的是 S3 conf：

```ini
[zspace]
type = s3
provider = Minio
env_auth = false
access_key_id = <username>
secret_access_key = <s3key>
region = cn-east-1
endpoint = http://127.0.0.1:<port>
```

---

## 2. 官方 CLI 实测（Wine 隔离 prefix）

```bash
WINEPREFIX=~/wine-mount-probe WINEDEBUG=-all \
  /opt/deepin-wine10-stable/bin/wine ZSpaceMountMonitor.exe --help
```

```
Usage:
  UZMountMonitor [flags]
  UZMountMonitor [command]

Available Commands:
  check / check-port-free / completion / config / disk / disk-free-space
  drivers / fuse / help / network / only-check-port-free / port / process
  random-free-port / rc / sys / version

Flags:
      --cache-dir string
      --cache-max-size SizeSuffix    (default off)
      --config-section string
      --home-dir string
      --host-name string
      --network-mode
      --no-fuse-mode
      --program string
      --rc-o stringArray
      --run-as-normal-user
      --show-hidden
      --use-filter
      --volume string
      --volume-label string
```

### 2.1 版本

```
Version:               1.1.1-20251111174657
Build With:            go version go1.20.14 windows/amd64
Build OS:              Windows 11 Pro 25H2 / 10.0.26100.6901
```

### 2.2 `config --add`

```bash
ZSpaceMountMonitor.exe config --add \
  --program='...\ZSpaceMount.exe' --home-dir='Z:\tmp\mountprobe' \
  --url=http://127.0.0.1:13600 --user=alice --pass=secret --obscure
```

```
{"status":200,"data":{"cmd":"...\\ZSpaceMount.exe --config=Z:\\tmp\\mountprobe\\mount.conf config create uzmount webdav url=http://127.0.0.1:13600 vendor=other user=alice pass=***","out":"..."},"error":""}
```

落盘 `/tmp/mountprobe/mount.conf`：

```ini
[uzmount]
type = webdav
url = http://127.0.0.1:13600
vendor = other
user = alice
pass = adlODkQBkbtUpeUm-OWodwfI7rHzTg
```

### 2.3 `check`

```bash
ZSpaceMountMonitor.exe check --cache-dir='Z:\tmp\mountprobe\cache' --volume='Z:'
```

```json
{"status":200,"data":{"can_read":true,"can_write":true,
"disk_free_space":5058326528,"drive_letter_used":["C","Z"],
"is_mount_point_used":false,"is_volume_valid":false,
"disk_file_system":"NTFS","drive_type":"Fixed",
"is_support_sparse":false,"is_support_sha2":true,
"fuse_status":null,"fuse_service_running":false,
"is_cloud":false,"is_root":true},"error":""}
```

### 2.4 其它子命令

```
disk --size --dir=...           -> {"status":200,"data":0,"error":""}
process --list --name=...       -> {"status":200,"data":[],"error":""}
port --free                     -> {"status":200,"data":42677,"error":""}
random-free-port                -> {"status":200,"data":44421,"error":""}
check-port-free 13600           -> {"status":200,"data":{"new_port":13600,"old_port":13600},"error":""}
rc --rc-addr=http://127.0.0.1:5999/mount/status
                                -> {"status":-1,"data":"","error":"request failed, ... Connection refused."}
fuse --status（无 WinFsp）       -> {"status":200,"data":[],"error":"FUSE not installed"}
```

### 2.5 ZSpaceMount.exe = rclone fork

```
rclone v1.70.2-20250702150924-windows-amd64
- os/type: windows
- go/version: go1.23.8
- go/tags: cmount
```

`config --add` 内部先执行 `ZSpaceMount.exe obscure secret`
→ `VOa0E3IPttm99xoMy3Q94H33Fy_BCQ`（与上游 rclone obscure 同格式）。

### 2.6 缓存上限收敛（mount_monitor.log）

```
[Warning] disk free space is less than cache max size 10Gi, set to: 2.404Gi
```

---

## 3. 从 monitor 反汇编/字符串得到的挂载参数

`ZSpaceMountMonitor.exe`（Go 1.20，pclntab 完整）中的字符串常量：

```
--config=  --cache-dir=  --cache-max-size=  --vfs-cache-mode=full
--vfs-cache-max-age=10m  --attr-timeout=5m  --file-perms=0777
--dir-cache-time=1m  --vfs-write-back=1s  --stats=5s
--volname=  --network-mode  --rc-addr=%s  --uninstall=
--filter-from=  --filter=- .*/  --filter=- .*
mount  .rw_test.tmp  mount.conf  mount.lock  mount.port
/mount/start  /mount/status  /mount/restart  /shutdown
[MOUNT] start mount, PID: %d, command: %s
[MOUNT] mount crashed, retry %d/%d after %s
```

根命令先构造参数数组 `["mount", "<section>:", "<mountpoint>"]`，再追加上述开关；
`--use-filter` 时追加 `--filter-from=<home>/filter.txt`，否则（非 `--show-hidden`）
追加 `--filter=- .*/` 与 `--filter=- .*`。

官方 fork 额外的 `--vfs-disk-space-info-from=` 及 `/mount/*` rc 端点在上游 rclone 中不存在，
由本仓库监控器兼容实现（见 README 偏离项）。

补充实测：上游 rclone 的 `POST /mount/listmounts` 对 CLI 启动的 `rclone mount` 返回
`{"mountPoints":[]}`，`POST /mount/status` 返回 404，因此 `is_mounting` 只能退化为
「rc 端口可连通」。

---

## 4. 自测证据

`native/mount/selftest/out/`（gitignore）中保留：

| 文件 | 内容 |
|---|---|
| `selftest.log` | 16 项断言与 PASS/FAIL |
| `01-config.json` | config --add 输出 |
| `02-check.json` | check 输出 |
| `03-mount.stdout/stderr` | 挂载进程输出 |
| `06-status.json` | `/mount/status` |
| `07-shutdown.json` | `/shutdown` |
| `08-disksize.json` | disk --size |
| `mount_monitor.log` | 监控器日志（含实际 rclone 命令行） |
| `mount.log` | rclone 日志（rc 监听、挂载通知） |

最近一次：`PASS=16 FAIL=0`。
