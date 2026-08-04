# Debug Helper AstroBox 插件

这是一个用于 AstroBox 连接与交互调试的 Go 插件（WIT Component 方案）。

- 使用 `wit-bindgen` 生成 Go 绑定
- 使用 `wasm-tools` 生成 component wasm
- 使用 Python 脚本完成初始化、构建与打包

## 这个插件是干什么的

它提供了一个调试 UI，帮助你快速验证 Interconnect 和 Transport 是否正常。
帮助开发者快速验证 Interconnect 交互是否达到预期。


## 目录结构

```text
.
├── bindings/                      # WIT 自动生成代码（不要手改）
├── build/                         # 中间产物
├── dist/                          # 最终产物
├── scripts/
│   ├── init.py                    # 初始化（submodule / bindings / tidy）
│   └── build_dist.py              # 构建与打包
├── src/                           # 业务代码（主要修改这里）
├── third_party/                   # 本地依赖（含 wit-bindgen replace）
├── tools/
│   └── wasi_snapshot_preview1.reactor.wasm
├── wit/                           # WIT 接口定义（submodule）
├── docs.md
├── main.go
├── manifest.json
└── README.md
```

## 环境要求

- Go `1.25.5+`（见 `go.mod`）
- Python 3
- `git`
- `wit-bindgen`
- `wasm-tools`

## 快速开始

```bash
# 1) 初始化：更新 submodule + 生成 bindings + go mod tidy
python scripts/init.py

# 2) 构建 dist
python scripts/build_dist.py

# 3) 打包 .abp（可选）
python scripts/build_dist.py --release --package
```

## 常用命令

```bash
# 仅重新生成 bindings（不更新 submodule、不 tidy）
python scripts/init.py --skip-submodule --skip-tidy

# release 构建（会附加 -trimpath）
python scripts/build_dist.py --release

# 指定 adapter 路径
python scripts/build_dist.py --adapter tools/wasi_snapshot_preview1.reactor.wasm

# 透传 go build 参数（示例）
python scripts/build_dist.py -- -tags custom_tag
```

## 构建产物

`python scripts/build_dist.py` 会生成：

- `build/core.wasm`
- `build/core-with-wit.wasm`
- `build/component.wasm`
- `dist/<manifest.entry>`（当前默认是 `dist/debug_helper_astrobox_v2_plugin.wasm`）
- `dist/manifest.json`
- `dist/icon.png`

`python scripts/build_dist.py --package` 额外生成：

- `dist/<manifest.name>.abp`（会自动做文件名安全处理）

## Adapter 说明

默认按以下优先级寻找 `wasi_snapshot_preview1.reactor.wasm`：

1. `--adapter <path>`
2. 环境变量 `WASI_PREVIEW1_REACTOR_ADAPTER`
3. `tools/wasi_snapshot_preview1.reactor.wasm`
4. `build/wasi_snapshot_preview1.reactor.wasm`（兼容旧路径）
5. 项目根目录下同名文件（兼容旧路径）

如果找不到会直接报错并退出。

## 重新生成 bindings（手动命令）

```bash
wit-bindgen go --world psys-world --pkg-name astroboxplugin/bindings --generate-stubs --out-dir bindings wit
```
