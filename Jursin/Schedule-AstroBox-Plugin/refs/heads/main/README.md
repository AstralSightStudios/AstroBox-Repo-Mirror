# 腕上课程表同步器

一个基于 Rust 和 WASI/Component Model 开发的 AstroBox V2 插件，用于推送课表配置文件到[腕上课程表快应用](https://github.com/Jursin/Schedule-Vela)。

## 环境
- Rust
- Python 3
- 安装 wasm32-wasip2 编译目标
   ```bash
   rustup target add wasm32-wasip2
   ```

## 构建
### Debug 构建到 dist 文件夹
```bash
python scripts/build_dist.py
```

### Release 构建到 dist 文件夹
```
python scripts/build_dist.py --release
```

### Release 构建并打包为 .abp 插件包
```
python scripts/build_dist.py --release --package
```

> [!tip]
> 构建产物会输出到 `dist/` 目录，包含编译后的 wasm 文件、`manifest.json` 和图标。
>
> 使用 `--package` 时会额外生成一个 `.abp` 文件，可直接通过 AstroBox 安装。

## 权限
- `device` - 访问设备信息
- `interconnect` - 与手环应用通信
- `thirdpartyapp` - 访问第三方应用
- `clipboard.write` - 复制日志到剪贴板

## 版本要求
- WASI 版本：2
- API 级别：3

## 许可证
本项目采用 [MIT](LICENSE) 许可证