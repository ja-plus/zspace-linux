---
name: zspace-safe
description: Inspect, unlock, lock, initialize, and back up a ZSpace safe box using business credentials.
---

# 保险箱（阶段性功能验收版）

先读 zspace-shared.md。命令为 `file safe --params '{"action":"..."}'`。
保险箱口令是独立的六位业务口令，不是 NAS 登录密码；登录仍只用 OAuth。

| action | 参数 | 行为 |
| --- | --- | --- |
| status | 可选 safe_dir | 只读状态；1 未创建、2 未打开、3 重置中、4 已打开 |
| tips | 可选 safe_dir | 只读口令提示，提示也可能包含私人信息 |
| open | password_env，可选 safe_dir | 用环境变量中的六位数字业务口令解锁，需 --yes |
| close | 可选 safe_dir | 锁定，关闭保险箱笔记数据库并卸载明文目录，需 --yes |
| init | password_env、pool，可选 tips:"" | 创建保险箱，pool 为 sataN/nvmeN/esataN，需 --yes |
| backup_status | 无 | 双备份状态、时间与下一次计划 |
| backup_start | 无 | 立即提交双备份，需 --yes，后续查询 backup_status |
| reset | all:true、sms_proof_env | 永久删除保险箱，高风险，需 --yes 和服务端签发的短信验证凭证 |

password_env 的值只在进程内读取，经 GET /key/pub 获取 NAS 公钥后，使用 RSA PKCS#1 v1.5
加密，再以 password 表单字段发送；不会将明文写入命令参数或报告。这里不接受账号登录密码。
生产代码只用 RSA 公钥加密，不保存或读取 NAS 私钥。
参考 [RustCrypto RSA API](https://docs.rs/rsa/0.9.8/rsa/)，项目实际锁定版本见 Cargo.lock。

safe_dir 仅用于快照保险箱，必须是快照列表给出的 `/<pool>/snapshot/<timestamp>/safe/my`
（可带 /plain）；不传表示当前账号保险箱。解锁后可使用 file list/read/download 访问 NAS 返回的路径，
笔记用 file notes location:"1" 及对应 safe_dir；NAS 仍校验用户与解锁条件。
reset 的 sms_proof_env 引用签名凭证，不接受短信数字验证码冒充；当前未实现从发送短信到取得签名凭证的完整流程。
不提供账号密码回退。reset 返回文件任务时，使用 file tasks status 查询终态，不能凭 HTTP 成功认定清理结束。

路由均为 POST form `/v2/file/safe/*`，backup_status/backup_start 对应 dwstatus/startdw。
证据：Web safeBox.js、views/safeBox/index.vue 与 login.vue、views/fileSnap/browseSnap.vue；
服务端 zfilev2/api/safe.go、model/safe.go；公钥由 zbasic/api/account.go 的 PubKey 提供。
快照状态、锁定、解锁还受快照路径条件限制；创建保险箱与双备份受设备存储池状态影响。
已按用户明确授权初始化当前账号保险箱，真机完成解锁、目录读取、保险箱笔记写入/回读/清理和最终锁定。业务 PIN 保存于仓库外私有交接文件，不写入报告。
本地 HTTP mock 验证公钥获取、密文表单与解密往返，未将本地测试当作真机解锁通过。

## 轻度加密目录

`file encrypt` 管理 NAS 的轻度加密目录，和保险箱是独立机制；不将目录锁定标志当作整盘加密保证。
所有动作需 `--yes`，业务口令只通过 password_env 引用环境变量，经同一 NAS 公钥加密后发送。

| action | 参数 | 行为 |
| --- | --- | --- |
| create | name、location、password_env，可选 encrypt_icon:true | 在 /<pool>/my/data 第一级创建目录，返回原始目录对象 |
| unlock/lock | path；unlock 另需 password_env | 解锁/锁定；用父目录 list 的 encrypted:1/2 核对，锁定时 file info 会被 NAS 拒绝 |
| release | path、password_env | 解除轻度加密，保留文件内容，按 destructive 处理 |
| password | path、password_env | 设置新业务口令；当前 NAS 接口按目录写权限授权，不要求旧口令，按 destructive 处理 |
| icon | path、enabled 布尔值 | 显示/隐藏加密图标，不改变目录锁定状态 |
| rename | path、new_name | 改名单级目录，不覆盖已存在目标 |

path 必须是 `/<pool>/my/data/<name>` 的一级目录，location 为 sataN/nvmeN/esataN，
不接受嵌套目录或团队空间路径。快照轻度加密目录解锁在当前后端要求账号密码二级验证，
本命令尚未接入该流程，不以业务目录口令替代账号验证。
创建/改名结果不明确时先查询父目录列表，按唯一名称核对，不能直接重试。
路由 POST form `/v2/encryptdir/new|unlock|lock|release|resetpasswd|seticon|rename`；
encrypt_icon 为 1/0，rename 的 new_name 映射为 newname。
证据：Web filev2.js、components/file/encryptFileDialog.vue、fileManage/actions.vue，
后端 api/encrypt_dir.go 和 model/encrypt_dir.go。
真机已核对全部七类动作、改密后旧口令拒绝及解除后内容一致；独立测试目录已解除并移入回收站。
