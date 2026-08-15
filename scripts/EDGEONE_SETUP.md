# EdgeOne Makers 控制台配置（Direct Upload 方案）

> **背景**：早期方案是让 EdgeOne Pages 直接连接 GitHub 子仓自动构建，但 EdgeOne
> 构建机（腾讯，国内）从 GitHub clone 1.3GB 内容太慢（实测 ~27 KiB/s），必然
> clone 超时失败。**正解是 EdgeOne Direct Upload**：由 GitHub Actions 服务器
> （访问 GitHub 快）负责同步与打包，再用 EdgeOne CLI 把 ZIP 直接上传部署，
> 完全绕开 EdgeOne 侧 git clone。以下配置只需在 EdgeOne 控制台做一次。

## 0. 免费版配额对照（已实测文档 132789）

| 项 | 免费版 | 本方案占用 |
|---|---|---|
| 项目数 | 40 | 3 |
| 部署次数 | 500 次/月 | ~90（母站 30 + 子站 2×30） |
| 总存储 | 5GB/站点 | ~1.3GB |
| redirects 规则 | 100 | 88 |
| 单文件 | 25MB | 镜像已按 25MiB 裁剪 |

Direct Upload 额外约束（已满足）：
- 单次上传：一个文件夹 / 一个 ZIP / 一个 HTML
- 项目资产 ≤ 20000 文件（mirror-01 约 2k、mirror-02 约 2k）
- 入口：ZIP 根必须有 `index.html`（子站 ZIP 已自动注入一个入口页）
- 文件名不得含 `\ / : * ? " < > |`

## 1. 创建 API Token（一次）

1. Makers 控制台 → **API Token** → 创建
2. 填描述（如 `astrobox-mirror`），选有效期（建议 90 天或 1 年）
3. 复制 token

## 2. 配置 GitHub Secret（一次）

1. 母仓 `AstralSightStudios/AstroBox-Repo-Mirror` → Settings → Secrets and
   variables → Actions
2. 新建 secret：Name `EDGEONE_API_TOKEN`，Value 上述 token
3. （可选）Actions variables 配 `EDGEONE_MAIN_PROJECT` = 母站项目名，
   默认 `astrobox-bootstrap`

## 3. 母站项目（mirror.abox.run）

1. 控制台 → 新建项目 → **直接上传（Direct Upload）** → 项目名
   `astrobox-bootstrap`（与 `EDGEONE_MAIN_PROJECT` 一致）
2. 先任意拖一个文件占位建项目（内容不重要，后面全由 Actions 覆盖）
3. 域名：绑定 `mirror.abox.run`（CNAME + 自动签发 SSL）

母站 `edgeone.json`（88 条 `/owner/*` → 子站 302 规则）由 Actions 每次
同步后自动重新生成并上传覆盖。

## 4. 子站项目（mirror-01 / mirror-02）

每个子仓建一个 Direct Upload 项目，**不要**连接 GitHub：

1. 控制台 → 新建项目 → **直接上传（Direct Upload）** → 项目名 `mirror-01`
2. 先任意拖一个文件占位建项目
3. 域名：绑定 `mirror-01.abox.run`
4. 重复建 `mirror-02`，绑定 `mirror-02.abox.run`

子站内容（按作者分仓的镜像文件，剔除 `.git`）由 Actions 打包上传覆盖。

## 5. 每日自动更新（无需人工）

母仓 Actions（`.github/workflows/mirror.yml`，每日 21:00 北京时间）：

- 全量同步上游 + 插件仓 → 按作者分仓（含超限自动均衡）→ force-push 到 org
  子仓（单 commit，保留 .git 最小，作为内容源备份）
- 重新生成 `edgeone.json` / `mapping.json` / `index.html` 并提交母仓
- 安装 EdgeOne CLI → 打包每个子仓 ZIP（剔除 `.git`，注入入口页）→
  `edgeone makers deploy` 上传到对应项目；母站打包 `index.html` +
  `edgeone.json` 上传到 `astrobox-bootstrap`

手动触发一次验证：母仓 → Actions → `Mirror AstroBox Repo` → Run workflow。

## 6. 分仓与自动均衡

- 作者（owner 顶层目录）永不跨仓拆分，映射持久化在 `mapping.json`，
  已有作者默认不搬仓（URL 稳定）
- 新作者进入当前最不满的仓；所有仓都放不下时自动新建 `mirror-NN`
- 当某个仓因作者内容增长超过 1GiB 时，自动均衡：把该仓内**最大的作者**
  整体移到最不满且装得下的仓（必要时新建仓），直到全部仓回落 1GiB 以内
- 单作者内容本身超过 1GiB 时无法均衡（作者不可拆分），保留原仓并打 WARN，
  此时该仓超限不影响其他仓，但需人工关注

均衡触发时会改变该作者的 302 目标，`edgeone.json` 同步更新，母站下次
部署后新路径生效，期间旧路径短暂 404 属正常。

## 7. 子仓推送 Secret（保留原配置）

`mirror.yml` 推送 org 子仓需要带 org 权限的 PAT：

1. GitHub → Settings → Developer settings → Personal access tokens → 新建（scope: `repo`）
2. 母仓 → Settings → Secrets → Actions → 新建 secret，Name:
   `ASTROBOX_ORG_TOKEN`，Value: 上述 PAT

无此 secret 时子仓推送会失败（本地 git 凭据只在手动跑时生效）。
