# SimpleFetch 手机端接入文档

## 概述

SimpleFetch 是快应用侧通过互联通道（interconnect）代理网络请求的模块。手机端充当网络代理：接收快应用的请求、执行真实 HTTP 请求、再把结果通过互联通道回传。

**通信流程**：快应用发送 `SF_REQUEST` → 手机端执行真实 HTTP 请求 → 手机端回传 `SF_RESPONSE` → 快应用拿到响应

握手成功后，快应用进入桥接模式（`global.NetworkStatus = 'bridge'`），所有 `fetch` 请求自动走 SimpleFetch，手机端无需关心业务层细节。

---

## 消息格式

所有消息为一个 JSON 对象，顶层结构固定为 `{ type, status, data }`：

- `type`：消息类型，SimpleFetch 相关均以 `SF_` 开头
- `status`：成功时为字符串 `'OK'`；失败时**直接填可读的错误原因字符串**（如 `'请求超时'`），不要用错误码
- `data`：消息体对象，字段因类型而异

互联底层会把这个对象作为消息体发送，手机端按同样的结构解析/回复即可。

---

## 消息类型一览

| 方向 | type | 说明 |
|------|------|------|
| 双向 | `SF_HANDSHAKE` | 发起握手（任一方均可发起） |
| 双向 | `SF_HANDSHAKE_ACK` | 握手确认（回复给发起方） |
| 快应用→手机 | `SF_PING` | 心跳 |
| 手机→快应用 | `SF_PONG` | 心跳回复 |
| 手机→快应用 | `SF_CLOSE_BRIDGE` | 手机要求关闭桥接 |
| 快应用→手机 | `SF_CLOSE_BRIDGE_ACK` | 关闭桥接确认 |
| 快应用→手机 | `SF_REQUEST` | 发起网络请求（普通或 SSE） |
| 手机→快应用 | `SF_RESPONSE` | 普通请求的响应（含分片） |
| 手机→快应用 | `SF_SSE_EVENT` | SSE 事件 |
| 手机→快应用 | `SF_SSE_END` | SSE 正常结束 |
| 手机→快应用 | `SF_SSE_ERROR` | SSE 出错 |
| 快应用→手机 | `SF_CLOSE` | 快应用主动关闭某个 SSE |

> SSE 请求**不会**收到 `SF_RESPONSE`，只有 `SF_SSE_*`。普通请求**只会**收到 `SF_RESPONSE`，不会收到 `SF_SSE_*`。

---

## 1. 握手

握手用于建立桥接，**任一方都可发起**，处理逻辑相同：收到 `SF_HANDSHAKE` 就回复 `SF_HANDSHAKE_ACK`。

- 手机端可在自身网络就绪后主动发起；
- 快应用端由用户在「偏好设置 → 连接桥接网络」手动发起。

**发起方发送：**
```json
{ "type": "SF_HANDSHAKE", "data": {} }
```

**接收方回复（必须带 status: 'OK'）：**
```json
{ "type": "SF_HANDSHAKE_ACK", "status": "OK", "data": {} }
```

快应用收到 `SF_HANDSHAKE_ACK`（或自己收到对端发来的 `SF_HANDSHAKE` 并回复 ACK 后）会：
- 设置 `global.NetworkStatus = 'bridge'`、`global.fetchAva = true`
- 启动 10 秒间隔的心跳

---

## 2. 心跳

握手成功后，快应用每 **10 秒**发送一次心跳。手机端必须回复；快应用发出 PING 后 **5 秒**内未收到 PONG 即判定超时，自动断开桥接。

**快应用发送：**
```json
{ "type": "SF_PING", "data": { "ts": 1700000000000 } }
```

**手机回复：**
```json
{ "type": "SF_PONG", "status": "OK", "data": { "ts": 1700000000000 } }
```

> `ts` 为毫秒时间戳，原样回传即可（快应用实际不校验该值，收到任意 `SF_PONG` 即视为存活）。

---

## 3. 手机端关闭桥接

手机端检测到自身网络不可用、用户手动关闭代理等场景，可主动要求快应用断开桥接。

**手机发送：**
```json
{ "type": "SF_CLOSE_BRIDGE", "status": "OK", "data": {} }
```

**快应用回复：**
```json
{ "type": "SF_CLOSE_BRIDGE_ACK", "status": "OK", "data": {} }
```

快应用随后会：停止心跳 → 拒绝所有待处理请求（reject 为「桥接连接断开」）→ 通知所有 SSE 出错 → 置 `NetworkStatus = 'none'` → 调用 `getDeviceInfo()` 刷新网络状态。

> 互联连接本身断开时（`onclose`），快应用也会自动执行上述清理，手机端无需额外发送 `SF_CLOSE_BRIDGE`。

---

## 4. 普通请求

### 4.1 快应用发送 `SF_REQUEST`

```json
{
  "type": "SF_REQUEST",
  "data": {
    "id": "sf_1",
    "url": "https://api.example.com/data",
    "method": "POST",
    "headers": { "Content-Type": "application/json" },
    "body": "{\"key\":\"value\"}",
    "sse": false,
    "timeout": 10000
  }
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 请求唯一 ID，响应必须原样回传，快应用靠它关联请求/响应 |
| `url` | string | 完整请求地址（已 encodeURI） |
| `method` | string | HTTP 方法，大写：`GET` / `POST` / `PUT` / `DELETE` 等 |
| `headers` | object | 请求头键值对；无请求头时为 `{}` |
| `body` | string\|null | 请求体字符串；GET 等无 body 时为 `null`。对象已被快应用 `JSON.stringify` |
| `sse` | boolean | 固定 `false`（普通请求） |
| `timeout` | number | 快应用侧的超时时间(ms)，建议手机端也以此作为本次 HTTP 请求的超时 |

### 4.2 响应：是否分片由 `totalChunks` 决定

快应用完全根据 `data.totalChunks` 判断如何解析 body：

- `totalChunks` 为 `0`（或缺失）→ **无分片**，`body` 是**原始字符串**，直接使用；
- `totalChunks > 0` → **分片**，所有分片的 `body` 拼起来是一整段 **base64 字符串**，快应用拼接后统一 `crypto.atob()` 解码。

分片阈值（建议 16KB）由手机端自行决定，快应用不关心。

#### A. 无分片（小数据）

```json
{
  "type": "SF_RESPONSE",
  "status": "OK",
  "data": {
    "id": "sf_1",
    "statusCode": 200,
    "headers": { "Content-Type": "application/json" },
    "body": "{\"code\":200,\"data\":{\"result\":\"success\"}}",
    "chunk": 0,
    "totalChunks": 0
  }
}
```

- `body` 为**未经编码的原始响应体字符串**。

#### B. 分片（大数据）

**关键：先对完整响应体做一次 base64 编码，再把这段 base64 字符串顺序切成 N 片**，每片放在对应消息的 `body` 里。不是「每片单独 base64」。

**第 1 片（chunk = 1，必须带 statusCode 和 headers）：**
```json
{
  "type": "SF_RESPONSE",
  "status": "OK",
  "data": {
    "id": "sf_1",
    "statusCode": 200,
    "headers": { "Content-Type": "application/json" },
    "body": "5L2g5aW9...(base64片段1)",
    "chunk": 1,
    "totalChunks": 5
  }
}
```

**第 2 ~ N 片（只需 id / body / chunk / totalChunks）：**
```json
{
  "type": "SF_RESPONSE",
  "status": "OK",
  "data": {
    "id": "sf_1",
    "body": "...(base64片段N)",
    "chunk": 2,
    "totalChunks": 5
  }
}
```

快应用在收到 `chunk === totalChunks` 时，按 `1..totalChunks` 顺序拼接所有 `body`，整体 base64 解码，再解析。

**分片规则：**
- `chunk` 从 **1** 开始，到 `totalChunks` 结束；
- 必须从第 1 片开始**按序**发送（快应用在收到 chunk=1 时才初始化缓存，先到的非首片会被丢弃）；
- 仅第 1 片需要 `statusCode`、`headers`，其余片可省略；
- 分片 body 是「整体 base64 后的子串」，不要每片单独编码。

### 4.3 响应体自动解析（与原生 fetch 对齐）

为与快应用原生 `@system.fetch` 行为一致，快应用会对 body 做自动 JSON 解析：

- `Content-Type` 含 `application/json` 或 `text/json` 时，自动 `JSON.parse(body)`；
- 解析失败时回退为原始字符串；
- 其他 Content-Type 一律作为字符串返回。

> 因此手机端应如实回传响应的 `Content-Type`。业务层（如本项目的 MingChen API）依赖 body 解析后是对象 `{ code, data, ... }`。

### 4.4 错误响应

请求失败时，`status` 直接填错误原因，`data.statusCode` 填 HTTP 状态码（网络层错误无状态码填 `0`）：

```json
{
  "type": "SF_RESPONSE",
  "status": "请求超时",
  "data": {
    "id": "sf_1",
    "statusCode": 0
  }
}
```

- 快应用会把该请求 reject 为 `{ code: statusCode || 0, data: status }`。
- `status` 必须是可读中文描述（会被上层捕获展示），不要填数字错误码。
- 常见：`请求超时`、`网络不可达`、`DNS解析失败`、`连接被拒绝`。
- HTTP 错误状态（如 500/404）也可以走 `status: 'OK'` + 真实 `statusCode`，由业务层按状态码处理；只有网络层/请求级失败才用非 OK 的 `status`。

---

## 5. SSE 流式请求

### 5.1 快应用发送 `SF_REQUEST`（sse: true）

```json
{
  "type": "SF_REQUEST",
  "data": {
    "id": "sf_2",
    "url": "https://api.example.com/stream",
    "method": "GET",
    "headers": {},
    "body": null,
    "sse": true,
    "timeout": 30000
  }
}
```

`sse: true` 标识流式请求。手机端建立 SSE 连接后，**不要回 `SF_RESPONSE`**，直接转发事件。

> 快应用在发出 SSE 请求后启动一个连接超时（即请求里的 `timeout`，默认 30000ms）。手机端必须在此时间内发来**第一个 `SF_SSE_EVENT`**，否则快应用判定 `SSE连接超时`。首个事件到达后不再有超时，连接保持到 END / ERROR / CLOSE。

### 5.2 转发事件 `SF_SSE_EVENT`

每收到一个服务端事件，转发一条：

```json
{
  "type": "SF_SSE_EVENT",
  "status": "OK",
  "data": {
    "id": "sf_2",
    "event": "message",
    "data": "事件数据内容"
  }
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 请求 ID（与 SF_REQUEST 一致） |
| `event` | string\|null | SSE 事件类型，默认/无名事件填 `"message"` |
| `data` | string | 事件数据 |

### 5.3 正常结束 `SF_SSE_END`

服务端关闭 SSE 流时发送：

```json
{ "type": "SF_SSE_END", "status": "OK", "data": { "id": "sf_2" } }
```

### 5.4 SSE 错误 `SF_SSE_ERROR`

连接建立后/流中出错时发送，`status` 填错误原因：

```json
{
  "type": "SF_SSE_ERROR",
  "status": "连接断开",
  "data": { "id": "sf_2" }
}
```

> 快应用优先取顶层 `status` 作为错误信息，其次取 `data.error`，最后兜底 `'SSE错误'`。

### 5.5 快应用主动关闭 `SF_CLOSE`

快应用调用 `close()` 时发送，手机端收到后应关闭对应的 SSE 连接：

```json
{ "type": "SF_CLOSE", "data": { "id": "sf_2" } }
```

---

## 6. 完整交互时序

### 普通请求（手机发起握手）

```
手机                          快应用
 │──── SF_HANDSHAKE ─────────→│  手机发起握手
 │←─── SF_HANDSHAKE_ACK ─────│  快应用确认，启动心跳
 │                              │
 │←─── SF_PING ───────────────│  每10秒心跳
 │──── SF_PONG ──────────────→│
 │                              │
 │←─── SF_REQUEST(sse=false) ─│  快应用发起请求
 │                              │  手机执行 HTTP 请求
 │──── SF_RESPONSE(0片) ─────→│  返回结果
 │                              │
 │←─── SF_PING ───────────────│  心跳持续
 │──── SF_PONG ──────────────→│
```

### 分片响应

```
手机                          快应用
 │←─── SF_REQUEST ────────────│
 │                              │  手机执行 HTTP，响应体较大
 │──── SF_RESPONSE 1/5 ──────→│  (base64 片段1，带 statusCode/headers)
 │──── SF_RESPONSE 2/5 ──────→│
 │──── SF_RESPONSE 3/5 ──────→│
 │──── SF_RESPONSE 4/5 ──────→│
 │──── SF_RESPONSE 5/5 ──────→│  快应用拼接→atob 解码→JSON 解析
```

### SSE

```
手机                          快应用
 │←─── SF_REQUEST(sse=true) ──│
 │                              │  手机建立 SSE 连接
 │──── SF_SSE_EVENT ──────────→│  首个事件（30s 内，清连接超时）
 │──── SF_SSE_EVENT ──────────→│
 │──── SF_SSE_END ────────────→│  流结束
 │                              │
 │  或快应用主动关闭：           │
 │←─── SF_CLOSE ──────────────│  手机关闭对应 SSE 连接
```

### 心跳超时断连

```
手机                          快应用
 │←─── SF_PING ───────────────│
 │  (5秒内未回 PONG)           │
 │                              │  判定超时：停止心跳、reject 待处理请求
 │                              │  NetworkStatus='none'、getDeviceInfo()
```

### 手机主动关闭桥接

```
手机                          快应用
 │──── SF_CLOSE_BRIDGE ───────→│
 │←─── SF_CLOSE_BRIDGE_ACK ────│
 │                              │  清理并刷新网络状态
```

---

## 7. 注意事项

1. **包名与签名**：手机端 App 的包名须与快应用 `src/manifest.json` 的 `package` 一致（`moe.mcns.ResonaUI`），且签名匹配，互联才能建立。
2. **心跳必须回复**：PONG 需在 5 秒内返回，否则桥接被断开。
3. **`id` 必须原样回传**：响应/SSE 消息里的 `id` 要和对应 `SF_REQUEST` 完全一致。
4. **分片判定只看 `totalChunks`**：`0` 表示原始字符串 body；`>0` 表示「整体 base64 后切片」。
5. **分片按序、首片完整**：从 chunk=1 开始顺序发，首片带 `statusCode`/`headers`。
6. **错误用中文描述**：`status` 直接填可读原因，不用数字码。
7. **如实回传 Content-Type**：快应用据此自动解析 JSON。
8. **SSE 首事件要及时**：必须在请求 `timeout` 内发来第一个 `SF_SSE_EVENT`，且 SSE 不回 `SF_RESPONSE`。
9. **互联断开自动清理**：通道本身断开时快应用会自动停用桥接，手机端无需补发关闭消息。
