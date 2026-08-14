# SmsForwarder-helper-astrobox-plugin

## 简介

SmsForwarder-helper-astrobox-plugin是一款 Astrobox 插件，用于管理手表端连接信息控制功能。

## 功能特性

- 支持设备互联通信
- 支持连接数据同步


## 技术栈

- **语言**: Rust
- **目标平台**: WASM32 WASI Preview2
- **框架**: Astrobox Plugin System (WIT)

## 项目结构

```
.
├── src/           # Rust 源码
│   ├── lib.rs     # 插件入口
│   ├── device.rs  # 设备相关
│   ├── logger.rs  # 日志初始化
│   ├── sync.rs    # 同步与互联
│   └── ui.rs      # UI 渲染与交互
├── wit/           # WIT 接口定义
├── .cargo/        # Cargo 构建配置
├── Cargo.toml     # Rust 依赖配置
└── manifest.json  # Astrobox 插件清单
```

## 构建

项目使用 `wasm32-wasip2` 目标进行构建：

```bash
cargo build --release
```

构建产物为 `smsforwarder_helper.wasm`，配合 `manifest.json` 与 `icon.png` 打包为 `.abp` 插件包。

## 许可证

MIT
