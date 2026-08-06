# 动态口令助手

## 简介

动态口令助手是一款 Astrobox 插件，用于管理手表端动态口令 TOTP 认证器，支持数据管理、同步、编辑等功能。

## 功能特性

- 管理手表端 TOTP 动态口令认证器
- 支持数据同步与互联通信
- 支持添加、编辑、删除认证器条目
- 实时刷新验证码与倒计时显示

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
│   ├── otp.rs     # TOTP 核心逻辑
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

构建产物为 `whistleo_otp.wasm`，配合 `manifest.json` 与 `icon.png` 打包为 `.abp` 插件包。

## 许可证

MIT