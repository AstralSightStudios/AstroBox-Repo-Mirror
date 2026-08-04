# shell-plus-plus-abv2-plugin

Shell++ 的 AstroBoxV2 插件项目。

## 环境

- Rust stable
- `wasm32-wasip2` target
- Python 3

## 构建

```bash
python3 scripts/build_dist.py
python3 scripts/build_dist.py --release --package
```

## CLI Deeplink

插件加载时会注册 AstroBox Deeplink 入口。AstroBox 桌面端和本插件已运行时，
可通过 `astrobox-cli open --url` 调用下列功能：

CLI 调用可在插件的 `Debug` 面板中随时开启或关闭；关闭后所有 Deeplink CLI
请求都会被拒绝。插件重新加载时默认开启。

- `status`
- `refresh-devices`
- `launch-app`
- `handshake`
- `request-screenshot-list`
- `request-raw-list`
- `sync-latest-raw`
- `exec`（JSON 参数 `cmd`，最长 2048 字节）
- `set-panel`（JSON 参数 `panel`）
- `clear-state`

例如：

```bash
npx --yes astrobox-cli open \
  --url 'astrobox://open?source=openPlugin&pluginName=Shell%2B%2B&data=handshake'
```

也可传入 URL 编码后的 JSON：

```text
{"action":"request-screenshot-list"}
```

执行 `ls`：

```bash
npx --yes astrobox-cli open \
  --url 'astrobox://open?source=openPlugin&pluginName=Shell%2B%2B&data=%7B%22action%22%3A%22exec%22%2C%22cmd%22%3A%22ls%22%7D'
```

需要在终端直接得到 `stdout`、`stderr` 和真实退出码时，使用随插件源码提供的
双向 CLI。它会临时监听随机的 `127.0.0.1` 端口，通过 Deeplink 投递命令，并由
插件在收到 QuickApp 的 `execResult` 后使用官方 `open-url` 接口回调：

```bash
python3 scripts/shellpp_cli.py ls /dev
python3 scripts/shellpp_cli.py --timeout 60 dmesg
```

回调只接受 loopback 地址，并带有每次随机生成的 token。CLI 会原样打印标准输出、
标准错误，并将手表命令的 `exitcode` 作为自身退出码。

CLI 只负责拉起 AstroBox 并投递事件，不会在终端等待插件的异步返回值。
执行结果会写入 Shell++ 插件的状态与日志界面。`exec` 复用原有设备通信、
请求关联和手表端安全检查；Deeplink 本身是本机可触发入口，请勿运行来源不明的链接。
未知 action 与无效参数会被拒绝。
