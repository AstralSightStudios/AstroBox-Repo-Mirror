# BandTOTP Sync

一个用于 [AstroBox NG](https://github.com/AstralSightStudios/AstroBox-NG) 的插件，将 TOTP 账户通过 `otpauth://totp/...` URI 同步到小米手环快应用。

本插件移植自 [BandTOTP-Android](https://github.com/leset0ng/BandTOTP-Android) 的同步逻辑：
- 解析 `otpauth://totp/issuer:account?secret=...` URI
- 获取已连接的穿戴设备
- 打开手环端 BandTOTP 快应用
- 通过 `interconnect::send-qaic-message` 将 JSON 数据发送到快应用

## 环境准备

1. 安装 Rust：https://www.rust-lang.org/learn/get-started
2. 安装 Python 3
3. 添加 wasm32-wasip2 目标：
   ```bash
   rustup target add wasm32-wasip2
   ```

## 构建

```bash
# Debug 构建到 dist 文件夹
python3 scripts/build_dist.py

# Release 构建到 dist 文件夹
python3 scripts/build_dist.py --release

# Release 构建并打包为 .abp 插件包
python3 scripts/build_dist.py --release --package
```

构建产物会输出到 `dist/` 目录，包含编译后的 wasm 文件、`manifest.json` 和图标。

## 使用

1. 在 AstroBox NG 中安装 `dist/BandTOTP Sync.abp`
2. 打开插件，输入或粘贴手环端 BandTOTP 快应用的包名（默认 `com.lst.bandtotp`）
3. 在文本框中每行粘贴一个 `otpauth://totp/...` URI，或点击「选择文件」读取 `.txt` 文件
4. 点击「打开手环应用」启动手环端 BandTOTP
5. 点击「同步到手环」发送数据

## 数据格式

发送到设备的 JSON 格式与 Android 端一致：

```json
{
  "list": [
    {
      "name": "issuer",
      "usr": "account",
      "key": "SECRET",
      "algorithm": "SHA1",
      "digits": 6,
      "period": 30
    }
  ]
}
```

## 许可证

MIT
