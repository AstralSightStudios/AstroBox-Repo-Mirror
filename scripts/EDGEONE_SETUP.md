# EdgeOne Makers 控制台配置

本仓库镜像内容已拆分为 org 子仓（`github.com/AstroBox-Repo-Mirror/mirror-01`、
`mirror-02`，每仓 ≤1GiB，作者不跨仓）。母仓只部署引导页 + `edgeone.json`
静态 302 规则。以下步骤在 EdgeOne Makers 控制台（pages.edgeone.ai）执行一次。

## 0. 免费版配额对照（已实测文档 132789）

| 项 | 免费版 | 本方案占用 |
|---|---|---|
| 项目数 | 40 | 3 |
| 构建次数 | 500 次/月 | ~90（母站 30 + 子站 2×30） |
| 总存储 | 5GB/站点 | ~1.3GB |
| redirects 规则 | 100 | 85 |
| 单文件 | 25MB | 镜像已按 25MiB 裁剪 |

## 1. 子站项目（mirror-01 / mirror-02）

每个子仓建一个 Pages 项目：

1. Makers 控制台 → 新建项目 → 连接 GitHub 仓库 `AstroBox-Repo-Mirror/mirror-01`
2. 构建配置：构建命令填 `rm -rf .git`（剔除 .git，减小产物与构建压力）
   - 构建命令字段：`rm -rf .git`
   - 输出目录：留空（整仓即产物）
3. 域名：绑定 `mirror-01.abox.run`（CNAME 到该项目提供的地址，自动签发 SSL）
4. 重复步骤建 `mirror-02`，绑定 `mirror-02.abox.run`

注意：子站项目不要配 `edgeone.json`（纯内容仓，无重定向需求）。

## 2. 母站项目（mirror.abox.run）

1. 新建项目 → 连接 GitHub 仓库 `AstralSightStudios/AstroBox-Repo-Mirror`
2. 构建配置：默认即可（仓库内已带 `edgeone.json`，构建器自动识别 302 规则）
3. 域名：绑定 `mirror.abox.run`（现有域名，若已绑定旧项目先解绑）
4. 构建命令（控制台里若之前配过 `rm -rf .git` 可保留，无害）

母站 `edgeone.json` 由 `scripts/mirror.py` 每次同步自动重新生成：
`/{owner}/*` → `https://mirror-XX.abox.run/{owner}/:splat` 302，共 85 条
（上限 100，作者数逼近 100 时需升级方案：边缘函数或按字母段分仓）。

## 3. 分仓与自动均衡

- 作者（owner 顶层目录）永不跨仓拆分，映射持久化在 `mapping.json`，
  已有作者默认不搬仓（URL 稳定）
- 新作者进入当前最不满的仓；所有仓都放不下时自动新建 `mirror-NN`
- 当某个仓因作者内容增长超过 1GiB 时，自动均衡：把该仓内**最大的作者**
  整体移到最不满且装得下的仓（必要时新建仓），直到全部仓回落 1GiB 以内
- 单作者内容本身超过 1GiB 时无法均衡（作者不可拆分），保留原仓并打 WARN，
  此时该仓超限不影响其他仓，但需人工关注

均衡触发时会改变该作者的 302 目标，`edgeone.json` 同步更新，EdgeOne
母站重建后（分钟级）新路径生效，期间旧路径短暂 404 属正常。

## 4. 每日自动构建

母仓 Actions（`.github/workflows/mirror.yml`，每日 21:00 北京时间）：

- 全量同步上游 + 插件仓 → 按作者分仓（含超限自动均衡）→ force-push 到 org
  子仓（单 commit，保持 .git 最小）
- 重新生成 `edgeone.json` / `mapping.json` / `index.html` 并提交母仓
- 母仓与子仓 push 都会触发各自 EdgeOne 项目重新构建部署

子仓不需要也不能自己拉取：内容源是上游作者仓库，同步只在母仓 Actions
统一执行后推送，这是唯一正确的更新形态。

## 5. Secrets

`mirror.yml` 推送 org 子仓需要带 org 权限的 PAT：

1. GitHub → Settings → Developer settings → Personal access tokens → 新建（scope: `repo`）
2. 母仓 `AstralSightStudios/AstroBox-Repo-Mirror` → Settings → Secrets and variables →
   Actions → 新建 secret，Name: `ASTROBOX_ORG_TOKEN`，Value: 上述 PAT

无此 secret 时子仓推送会失败（本地 git 凭据只在手动跑时生效）。
