# QT Vela AstroBox V2 Plugin

用于向 QT Vela QuickApp 导入二维码的 AstroBox V2 插件。

## 使用

1. 在 AstroBox V2 中连接手表并安装本插件。
2. 在手表打开 QT Vela 的同步页面。
3. 点击“刷新设备”，授予设备与通信权限。
4. 填写标题、可选备注，并选择二维码图片识别或输入二维码内容。
5. 也可以粘贴 QT Web 工具生成的同步 JSON。
6. 点击“同步到手表”。

QuickApp 项目：
<https://github.com/IKUN-CXKPRO/QT-Vela-QuickApp>

## 构建

```bash
python3 scripts/build_dist.py --release --package
```

## 许可

本项目使用 [Apache License 2.0](LICENSE)。
