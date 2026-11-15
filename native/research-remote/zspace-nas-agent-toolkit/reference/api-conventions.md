# CLI 约定

## 调用形态

```text
zspace-cli [全局参数] <domain> <action> --params '<JSON>' [--yes]
zspace-cli [全局参数] tool <workbuddy-name> --params '<JSON>' [--yes]
zspace-cli [全局参数] raw <GET|HEAD|POST> <path> --params '<JSON>'
```

业务参数统一放在单个 JSON 对象中。数组表单自动编码为 `key[]`；布尔值编码为 `1/0`。NAS 路径必须
以 `/` 开头，且不得含 `..` 或反斜杠。

## 输出

成功写 stdout：

```json
{"ok":true,"profile":"home","data":{}}
```

失败写 stderr 并以非零退出：

```json
{"ok":false,"error":{"code":"E_POLICY","message":"..."}}
```

命令名、选项或参数值无法解析时同样只输出一个 JSON 错误对象，错误码为 `E_PARAM`、退出码为 2；
`--help` 与 `--version` 是例外，正常输出人类可读文本并以 0 退出。解析错误不会回显原始参数值，避免
误把凭证写入终端或日志。包装脚本缺少二进制或不支持当前平台时分别返回 `E_INSTALL` 或
`E_UNSUPPORTED_PLATFORM` JSON 错误。

输出层递归遮蔽包含 token、password、auth_code、Authorization 或 Cookie 的字段。错误信息不会包含
请求头、表单或响应原文。

`E_PERMISSION` 表示 NAS 拒绝当前授权访问，`E_NOT_FOUND` 表示详情对象不存在；这两类不再以带内部
服务名的 `E_API` 或 `ok:true` 空对象返回。

## 风险

- `read`：无需 `--yes`。
- `write`：先确认目标与影响，再传 `--yes`。
- `destructive`：列出完整对象、说明可恢复性并取得明确同意，再传 `--yes`。

混合工具按实际 `action` 门禁：`recyclebin list`、`share list|statics`、`labels list|files`、
`file_tasks list|status` 与 `media_convert list|status` 为只读，不传 `--yes`；其余写入或高风险
action 仍必须确认。`media_convert wipe` 是批量不可逆操作，除 `--yes` 外还必须显式传 `all:true`；
后端不支持按单个 `task_id` 删除。能力目录中混合工具的风险表示其最高风险。

`raw` 的方法与路径都要命中白名单。例如标签列表必须使用
`raw POST /v2/labels/alllabels`，不是 GET。Windows Git Bash/MSYS 若把 `/v2/...` 改写为盘符路径，
CLI 会在后缀精确命中白名单时安全恢复；其他情况会提示使用 `MSYS_NO_PATHCONV=1`。

## 本地文件

上传和下载只允许 `--local-root` 下的路径，默认当前工作目录；相对 `local_path` 始终相对
`--local-root` 解析。上传默认在目标重名时返回 `E_EXISTS`，不会静默改名；覆盖需同时传
`overwrite:true` 与 `--yes`，显式 `rename:1` 才允许自动改名，并以响应中的 `path` 为实际路径。
上传/write 的 `rename` 接受整数 0/1/2/3 或对应数字字符串，不接受布尔值，语义为 `0=失败、1=自动改名、2=跳过、3=覆盖`，不得与含义冲突的 `overwrite:true`
同时使用。空文件也会向 NAS 发出创建请求并校验父目录与重名策略。

copy/move 的设备端 `rename:0` 可能覆盖既有文件，CLI 因此拒绝该值；仅接受 1=自动改名、2=跳过、3=明确覆盖，默认 1。`to` 必须为已存在目录。

上传按 8 MiB 分片并使用稳定 upload ID 尝试续传，只从 NAS 已确认的完整分片边界恢复；输出中的
`remote_partial_size`、`resumed_from` 与 `resume_reason` 说明判断结果。只有 NAS 明确返回临时文件不存在
时才从 0 开始；查询超时、权限或异常响应不会被静默吞掉。下载按 Range 分片，先写同目录临时文件，
成功后再替换目标，失败不会留下 0 字节目标。`download_file` 覆盖现有文件同样需
`overwrite:true` 与 `--yes`。
