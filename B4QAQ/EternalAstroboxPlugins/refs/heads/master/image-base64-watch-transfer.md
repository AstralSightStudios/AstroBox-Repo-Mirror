# 图片 Base64 传输到手环：Android → Vela 全链路示例

本文以当前 `WallpaperLooper` 项目的实现为准，说明一张图片从 Android 端读取、压缩、Base64 编码、分片，通过 `@system.interconnect` 发送到 Vela 手环，再解码并落盘的完整链路。

## 1. 链路总览

```text
Android Uri/File/Bitmap
    │
    ├─ Bitmap.compress(JPEG/WEBP)
    ├─ Base64.encodeToString(bytes, NO_WRAP)
    ├─ 按 4 字符对齐切片
    └─ MessageApi.sendMessage(JSON)
             │  Bluetooth / interconnect
             ▼
Vela interconnectHelper
    │
    ├─ 识别 action=SEND_FILE
    ├─ normalizeMessage(a/n/i/e/s/d/t)
    ├─ decodeBase64Chunk(d) → Uint8Array
    └─ file.writeArrayBuffer(append)
             │
             ├─ 非 EOF：FILE_CHUNK_SAVED(fileName,index)
             └─ EOF：FILE_SAVED(fileName)
```

图片传输至少需要保证：所有分片按顺序到达、每个分片的 Base64 长度（除最后一片外）是 4 的倍数、首片覆盖写入而后续分片追加写入。

## 2. 消息协议

Android 当前发送的紧凑 JSON 如下（字段名不能改）：

```json
{
  "a": "SEND_FILE",
  "n": "wallpaper.jpg",
  "i": 0,
  "e": 0,
  "s": 1,
  "d": "/9j/4AAQSkZJRgABAQ...",
  "t": "image"
}
```

| 字段 | 含义 |
|---|---|
| `a` | action，图片数据固定为 `SEND_FILE` |
| `n` | 文件名；普通图片使用纯文件名，任务传输可使用 `taskId/relativePath` |
| `i` | 从 0 开始的分片序号 |
| `e` | 是否最后一片，`1` 表示 EOF |
| `s` | 是否首片，`1` 表示开始新文件 |
| `d` | Base64 分片正文，不带 `data:image/...;base64,` 前缀 |
| `t` | 文件类型，图片为 `image`；设置壁纸时使用 `image_set_bg` |

开始任务目录传输时，还会先发送 `TRANSFER_SESSION_START`，每个文件发送 `TRANSFER_FILE_BEGIN`，但 `SEND_FILE` 的解码和写入逻辑相同。

## 3. Android：图片转 Base64 并入队

### 3.1 Bitmap 转 Base64

项目已有 `BitmapUtils.bitmapToBase64`，内部使用 `Bitmap.compress` 和 `Base64.NO_WRAP`：

```java
Bitmap bitmap = BitmapFactory.decodeStream(inputStream);
String base64 = BitmapUtils.bitmapToBase64(
        bitmap,
        Bitmap.CompressFormat.JPEG,
        80
);
if (base64 == null || base64.isEmpty()) {
    throw new IOException("图片压缩或 Base64 编码失败");
}
```

如果已经有本地图片文件，可直接读取二进制再编码：

```java
byte[] bytes = Files.readAllBytes(imageFile.toPath());
String base64 = Base64.encodeToString(bytes, Base64.NO_WRAP);
```

不要使用 `Base64.DEFAULT`，它可能插入换行；也不要把 MIME 前缀拼进 `d` 字段。

### 3.2 Base64 对齐分片

```java
private static final int CHUNK_SIZE = 16 * 1024;

List<String> chunks = StringUtils.splitBase64Aligned(base64, CHUNK_SIZE);
for (int index = 0; index < chunks.size(); index++) {
    boolean first = index == 0;
    boolean eof = index == chunks.size() - 1;
    WearFileMsg msg = new WearFileMsg(
            "wallpaper.jpg", index, chunks.get(index), first, eof, "image"
    );
    // 实际项目将 msg 放入 MiWearUtils.mFileMsgList，由 sendFile() 顺序发送
}
```

`StringUtils.splitBase64Aligned` 会把最大长度向下对齐到 4 的倍数。最后一片可以短于 4 的倍数，因为它包含完整 Base64 字符串的结尾和 padding。

### 3.3 使用项目现成入口（推荐）

对于本地图片文件，推荐交给 `MiWearUtils.sendLocalFiles`，它已经完成读取、编码、分片、队列、重试和确认：

```java
File imageFile = new File(getCacheDir(), "wallpaper.jpg");
MiWearUtils.sendLocalFiles(
        new File[]{imageFile},
        "",                 // image 类型使用纯文件名
        "image_set_bg",     // 普通图片改为 "image"
        null,
        () -> Log.i("Transfer", "图片全部发送完成")
);
```

该方法内部等价于：`Base64.NO_WRAP` → `splitBase64Aligned` → 构造 `WearFileMsg` → `buildCompactTransferMessage` → `MessageApi.sendMessage`。

## 4. Android：构造并发送一片

以下代码展示协议的最小发送单元；生产代码应继续使用 `MiWearUtils.sendLocalFiles` 的队列和确认机制：

```java
private void sendChunk(Node node, WearFileMsg msg) {
    JsonObject payload = new JsonObject();
    payload.addProperty("a", "SEND_FILE");
    payload.addProperty("n", msg.fileName);
    payload.addProperty("i", msg.index);
    payload.addProperty("e", msg.eof ? 1 : 0);
    payload.addProperty("s", msg.start ? 1 : 0);
    payload.addProperty("d", msg.base64);
    payload.addProperty("t", msg.fileType == null ? "image" : msg.fileType);

    Wearable.getMessageApi(Utils.getApp())
            .sendMessage(node.id, payload.toString().getBytes(StandardCharsets.UTF_8))
            .addOnFailureListener(error -> Log.e("Transfer", "分片发送失败", error));
}
```

项目中的 `MiWearUtils.sendFile()` 会在图片文件的每一片后等待 `FILE_CHUNK_SAVED`，EOF 片还会等待 `FILE_SAVED`，从而避免 Vela 异步追加写入乱序。

## 5. Vela：接收、解码、追加写入

### 5.1 注册 interconnect listener

```javascript
global.registerInterconnectListener("transfer-page", {
  actions: ["SEND_FILE"],
  onMessage: (message) => {
    saveFile(transferUtils.normalizeMessage(message))
  }
})
```

`normalizeMessage` 同时兼容完整字段（`action/param`）和 Android 当前的紧凑字段（`a/n/i/e/s/d/t`）。

### 5.2 Base64 解码并写文件

```javascript
function saveFile(cmd) {
  const writePath = "internal://files//main/" + cmd.fileName
  const bytes = utils.atob(cmd.base64, { trusted: true })

  file.writeArrayBuffer({
    uri: writePath,
    buffer: bytes,
    append: cmd.index !== 0,
    success: () => {
      if (!cmd.eof) {
        global.sendInterconnect("FILE_CHUNK_SAVED", JSON.stringify({
          fileName: cmd.fileName,
          index: cmd.index
        }))
        return
      }
      global.sendInterconnect("FILE_SAVED", cmd.fileName)
    },
    fail: (data, code) => {
      console.error("图片写入失败", code, data)
    }
  })
}
```

项目真实实现还会在首片前创建目录、按 `image_set_bg` 保存待切换壁纸路径、更新父目录索引，并通过 `transferUtils.shouldAckEachChunk("image", fileName)` 判断是否逐片确认。

### 5.3 自定义解码器

项目中的 `utils.atob` 是对解码器的统一封装，代码位于 `src/common/utils/utils.js`：

```javascript
import transferUtils from "./transferUtils.js"

const utils = {
  atob: (base64, options) => {
    return transferUtils.decodeBase64Chunk(base64, options)
  }
}

export default utils
```

页面调用 `utils.atob`，实际进入 `transferUtils.decodeBase64Chunk`，返回 `Uint8Array`：

```javascript
const bytes = utils.atob(base64Chunk, { trusted: true })
// bytes 可直接作为 @system.file.writeArrayBuffer 的 buffer
```

解码器的核心实现如下。`trusted: true` 用于手机端已经校验格式的传输分片；不可信输入可省略该选项，函数会先移除非法 Base64 字符：

```javascript
const BASE64_CHARS =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
const INVALID_BASE64_CHARS = /[^A-Za-z0-9+/=]/g
let base64DecodeTable = null

function getBase64DecodeTable() {
  if (base64DecodeTable) return base64DecodeTable

  const table = new Int16Array(256)
  table.fill(-1)
  for (let i = 0; i < BASE64_CHARS.length; i++) {
    table[BASE64_CHARS.charCodeAt(i)] = i
  }
  base64DecodeTable = table
  return table
}

function decodeBase64Chunk(base64, options = {}) {
  const trusted = options.trusted === true
  const input = trusted ? base64 : base64.replace(INVALID_BASE64_CHARS, "")
  if (!input) return new Uint8Array(0)

  const table = getBase64DecodeTable()
  const padding = input.endsWith("=")
    ? (input.endsWith("==") ? 2 : 1)
    : 0
  const output = new Uint8Array((input.length >> 2) * 3 - padding)

  let outIndex = 0
  for (let i = 0; i < input.length; i += 4) {
    const c1 = table[input.charCodeAt(i)]
    const c2 = table[input.charCodeAt(i + 1)]
    const c3Code = input.charCodeAt(i + 2)
    const c4Code = input.charCodeAt(i + 3)
    const c3 = c3Code === 61 ? 0 : table[c3Code]
    const c4 = c4Code === 61 ? 0 : table[c4Code]
    const value = (c1 << 18) | (c2 << 12) | (c3 << 6) | c4

    output[outIndex++] = (value >> 16) & 0xff
    if (outIndex < output.length) output[outIndex++] = (value >> 8) & 0xff
    if (outIndex < output.length) output[outIndex++] = value & 0xff
  }
  return output
}
```

其中 `getBase64DecodeTable()` 为 `0-255` 字节建立字符索引表；完整实现可直接参考 `WearableApp/WallpaperLooper/src/common/utils/transferUtils.js`。

发送端必须保证分片边界对齐，因此 Vela 可以对每片独立解码后追加二进制，而不需要先拼接完整 Base64 字符串。

## 6. 完整时序（含确认）

1. 手环进入 `/pages/transfer`，发送 `TRANSFER_READY`。
2. Android 收到 ready，调用 `sendLocalFiles`，逐文件建立 `WearFileMsg` 队列。
3. Android 发送 `SEND_FILE(i=0,s=1,e=0)`。
4. Vela 解码并以 `append:false` 写入，成功后回 `FILE_CHUNK_SAVED(fileName,index)`。
5. Android 收到确认后发送下一片；EOF 片写入完成后 Vela 回 `FILE_SAVED(fileName)`。
6. Android 队列清空后可发送 `TRANSFER_FINISH`/`TRANSFER_COMPLETE`，Vela 完成 UI 收尾。

连接断开时不要重新编码已发送内容；保留未确认的 `WearFileMsg`，重连后从该分片继续。项目中的 `TransferTaskManager`、`WearTransferRecoveryManager` 已负责这部分恢复流程。

## 7. 常见问题与排查

- **解码失败或图片损坏**：检查是否使用 `NO_WRAP`，是否把 Base64 MIME 前缀混入 `d`，以及中间分片是否为 4 的倍数。
- **只有第一片能打开**：Vela 后续写入必须使用 `append:true`，且 Android `index` 必须从 0 连续递增。
- **图片覆盖了旧文件**：首片使用 `append:false`；任务恢复/重传前由 `TRANSFER_FILE_BEGIN` 清理或显式覆盖目标文件。
- **消息过大或丢包**：降低 `CHUNK_SIZE`，保留图片逐片确认；不要同时无确认发送多个图片分片。
- **路径找不到**：普通图片使用纯文件名；任务传输使用 `taskId/relativePath`，并先发送 `TRANSFER_SESSION_START` 与 `TRANSFER_FILE_BEGIN`。

## 8. 代码对应位置

- Android Base64 与分片：`WallpaperLooper/app/src/main/java/com/xsli/wallpaperlooper/utils/BitmapUtils.java`、`StringUtils.java`、`MiWearUtils.java`
- Android 消息模型：`WallpaperLooper/app/src/main/java/com/xsli/wallpaperlooper/modal/WearFileMsg.java`
- Vela 解码与消息归一化：`WearableApp/WallpaperLooper/src/common/utils/transferUtils.js`
- Vela 接收与落盘：`WearableApp/WallpaperLooper/src/pages/transfer/transfer.js`
- Vela 通信封装：`WearableApp/WallpaperLooper/src/common/utils/interconnectHelper.js`

## 9. 集成方可复制的 Android 完整模板

下面模板不依赖项目的 `WearFileMsg` 和 `StringUtils`，适合集成方先跑通协议。接入项目现有代码时，建议替换为 `MiWearUtils.sendLocalFiles`，因为它额外处理了断线恢复、超时重试和任务队列。

### 9.1 图片读取、编码和分片

```java
import android.content.ContentResolver;
import android.content.Context;
import android.net.Uri;
import android.util.Base64;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.List;

public final class ImageBase64Encoder {
    private ImageBase64Encoder() {}

    public static String encodeFile(File file) throws IOException {
        byte[] bytes = java.nio.file.Files.readAllBytes(file.toPath());
        return Base64.encodeToString(bytes, Base64.NO_WRAP);
    }

    public static String encodeUri(Context context, Uri uri) throws IOException {
        ContentResolver resolver = context.getContentResolver();
        try (InputStream input = resolver.openInputStream(uri)) {
            if (input == null) throw new IOException("无法打开图片 Uri: " + uri);
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            byte[] buffer = new byte[16 * 1024];
            int count;
            while ((count = input.read(buffer)) != -1) {
                output.write(buffer, 0, count);
            }
            return Base64.encodeToString(output.toByteArray(), Base64.NO_WRAP);
        }
    }

    public static List<String> splitAligned(String base64, int maxLength) {
        if (base64 == null || base64.isEmpty()) return new ArrayList<>();
        int sliceLength = Math.max(4, (maxLength / 4) * 4);
        List<String> result = new ArrayList<>();
        for (int start = 0; start < base64.length(); start += sliceLength) {
            result.add(base64.substring(start,
                    Math.min(start + sliceLength, base64.length())));
        }
        return result;
    }
}
```

如果源文件来自相册 `content://` Uri，优先使用 `encodeUri`，不要直接把 Uri 字符串当作文件路径传给 `File`。

### 9.2 协议消息对象和发送器

```java
import com.google.android.gms.tasks.Task;
import com.google.android.gms.wearable.MessageClient;
import com.google.android.gms.wearable.Node;
import com.google.android.gms.wearable.Wearable;
import com.google.gson.JsonObject;

import android.content.Context;

import java.nio.charset.StandardCharsets;
import java.util.List;

public final class ImageChunkSender {
    public static final int DEFAULT_CHUNK_SIZE = 16 * 1024;

    private final MessageClient messageClient;
    private final Node node;

    public ImageChunkSender(Context context, Node node) {
        this.messageClient = Wearable.getMessageClient(context.getApplicationContext());
        this.node = node;
    }

    public Task<Integer> sendImage(String fileName, String base64) {
        return sendImage(fileName, base64, "image", DEFAULT_CHUNK_SIZE);
    }

    public Task<Integer> sendImage(
            String fileName, String base64, String fileType, int chunkSize) {
        List<String> chunks = ImageBase64Encoder.splitAligned(base64, chunkSize);
        if (chunks.isEmpty()) {
            throw new IllegalArgumentException("图片 Base64 为空");
        }

        // 此示例只负责把消息全部提交给 MessageClient；生产环境应在收到
        // FILE_CHUNK_SAVED 后再发送下一片，参见 9.3。
        Task<Integer> lastTask = null;
        for (int index = 0; index < chunks.size(); index++) {
            JsonObject message = new JsonObject();
            message.addProperty("a", "SEND_FILE");
            message.addProperty("n", fileName);
            message.addProperty("i", index);
            message.addProperty("e", index == chunks.size() - 1 ? 1 : 0);
            message.addProperty("s", index == 0 ? 1 : 0);
            message.addProperty("d", chunks.get(index));
            message.addProperty("t", fileType == null ? "image" : fileType);

            lastTask = messageClient.sendMessage(
                    node.getId(),
                    message.toString().getBytes(StandardCharsets.UTF_8)
            );
        }
        return lastTask;
    }
}
```

### 9.3 带 ACK 的顺序发送器（推荐）

图片分片不建议并发发送。下面是发送器的核心状态机：收到 `FILE_CHUNK_SAVED` 才推进下一片，收到 `FILE_SAVED` 才结束当前文件。

```java
public final class OrderedImageTransfer {
    public interface Transport {
        void send(String action, String param, Runnable success, Runnable failure);
    }

    private final Transport transport;
    private List<String> chunks;
    private String fileName;
    private String fileType;
    private int nextIndex;
    private Runnable onComplete;

    public OrderedImageTransfer(Transport transport) {
        this.transport = transport;
    }

    public void start(String fileName, String base64, String fileType,
                      Runnable onComplete) {
        this.fileName = fileName;
        this.fileType = fileType == null ? "image" : fileType;
        this.chunks = ImageBase64Encoder.splitAligned(base64, 16 * 1024);
        this.nextIndex = 0;
        this.onComplete = onComplete;
        sendNext();
    }

    // 在 Android 的 interconnect/message 回调中调用此方法。
    public void onMessage(String action, String param) {
        if ("FILE_CHUNK_SAVED".equals(action)) {
            sendNext();
        } else if ("FILE_SAVED".equals(action)
                && fileName.equals(param)) {
            if (onComplete != null) onComplete.run();
        }
    }

    private void sendNext() {
        if (chunks == null || nextIndex >= chunks.size()) return;
        final int index = nextIndex++;
        JsonObject payload = new JsonObject();
        payload.addProperty("a", "SEND_FILE");
        payload.addProperty("n", fileName);
        payload.addProperty("i", index);
        payload.addProperty("e", index == chunks.size() - 1 ? 1 : 0);
        payload.addProperty("s", index == 0 ? 1 : 0);
        payload.addProperty("d", chunks.get(index));
        payload.addProperty("t", fileType);
        transport.send("SEND_FILE", payload.toString(), () -> {}, this::reset);
    }

    private void reset() {
        chunks = null;
        nextIndex = 0;
    }
}
```

实际项目中 `FILE_CHUNK_SAVED` 的 `param` 是 JSON：`{"fileName":"wallpaper.jpg","index":0}`，应先解析并校验文件名和序号，再调用 `sendNext()`；不能只依据任意 ACK 推进队列。

## 10. 集成方可复制的 Vela 接收模板

下面示例把 `utils.atob`、目录创建、首片覆盖、后续追加和 ACK 组合在一起。`@system.file` 的 URI 必须使用 Vela 支持的 `internal://` 路径。

```javascript
import file from "@system.file"
import transferUtils from "../../common/utils/transferUtils.js"
import utils from "../../common/utils/utils.js"

const LISTENER_ID = "image-transfer-integration"
const ROOT_URI = "internal://files//main/"

function normalizeFileName(name) {
  // 普通图片只允许文件名；任务传输需要自行替换为受控的 taskId/relativePath 解析器。
  return (name || "").replace(/^\/+/, "").replace(/\\/g, "/")
}

function ensureDirectory(uri) {
  return new Promise((resolve) => {
    const slash = uri.lastIndexOf("/")
    const dirUri = slash >= 0 ? uri.substring(0, slash + 1) : ROOT_URI
    file.mkdir({
      uri: dirUri,
      recursive: true,
      success: () => resolve(),
      fail: () => resolve() // 目录已存在时也继续写入
    })
  })
}

function writeChunk(cmd) {
  const safeName = normalizeFileName(cmd.fileName)
  if (!safeName || safeName.indexOf("..") >= 0 || !cmd.base64) return

  const writeUri = ROOT_URI + safeName
  const bytes = utils.atob(cmd.base64, { trusted: true })
  ensureDirectory(writeUri).then(() => {
    file.writeArrayBuffer({
      uri: writeUri,
      buffer: bytes,
      append: cmd.index !== 0,
      success: () => {
        if (cmd.eof) {
          global.sendInterconnect("FILE_SAVED", cmd.fileName)
        } else {
          global.sendInterconnect(
            "FILE_CHUNK_SAVED",
            transferUtils.buildChunkAckPayload(cmd.fileName, cmd.index)
          )
        }
      },
      fail: (data, code) => {
        console.error("writeArrayBuffer failed", code, data)
        global.sendInterconnect("FILE_WRITE_FAILED", JSON.stringify({
          fileName: cmd.fileName,
          index: cmd.index,
          code: code || -1
        }))
      }
    })
  })
}

export function registerImageTransfer() {
  global.registerInterconnectListener(LISTENER_ID, {
    actions: ["SEND_FILE"],
    onMessage: (message) => {
      const cmd = transferUtils.normalizeMessage(message)
      if (cmd.action !== "SEND_FILE") return false
      writeChunk(cmd)
      return true
    }
  })
}

export function unregisterImageTransfer() {
  global.unregisterInterconnectListener(LISTENER_ID)
}
```

正式接入时建议复用项目 `pages/transfer/transfer.js` 的 `prepareTransferTarget` 和 `resolveTransferContext`，因为它们还处理 `.ctpic`、缩略图、任务目录、索引文件和 `image_set_bg` 壁纸切换。

## 11. Vela 端 `normalizeMessage` 最小实现

如果集成方没有复用项目的 `transferUtils.js`，至少需要保留以下兼容逻辑：

```javascript
export function normalizeMessage(message) {
  const action = message.action || message.a || ""
  if (message.param) {
    const payload = typeof message.param === "string"
      ? JSON.parse(message.param)
      : message.param
    return {
      action,
      fileName: payload.fileName || "",
      index: Number(payload.index || 0),
      eof: payload.eof === true,
      start: payload.start === true,
      base64: payload.base64 || "",
      fileType: payload.fileType || "image"
    }
  }
  return {
    action,
    fileName: message.n || "",
    index: Number(message.i || 0),
    eof: message.e === true || message.e === 1,
    start: message.s === true || message.s === 1,
    base64: message.d || "",
    fileType: message.t || "image"
  }
}
```

## 12. 接入前检查清单

- Android 使用 `Base64.NO_WRAP`，`d` 不带 MIME 前缀。
- 除最后一片外，Base64 分片长度是 4 的倍数。
- `i` 从 0 开始连续递增，首片 `s=1`，末片 `e=1`。
- Vela 首片 `append:false`，后续片 `append:true`。
- Vela 只有写入成功后才回 ACK；Android 校验 ACK 中的 `fileName/index`。
- 图片分片按顺序发送，避免多个 `writeArrayBuffer` 并发追加。
- 文件名经过路径校验，禁止 `..`、绝对路径和未授权目录。
- 大图优先缩放或降低 JPEG/WEBP quality，避免 Android 和 Vela 内存峰值过高。
- 生产环境保留超时、重试、断线恢复和重复 ACK 幂等处理。
