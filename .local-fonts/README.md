# .local-fonts — 专有字体，禁止分发

**本目录整体不入库、不打包、不随任何产物分发。**

## 内容

| 文件 | 说明 |
| --- | --- |
| `gaugemono.ttf` | Gauge Mono（Ookla 专有仪表字体）本地校准副本，8 360 B |

来源：`C:\oss\Project\speedtest\public\fonts\gaugemono.woff2`（4 240 B），
经 fontTools 去 woff2 封装转为 ttf，轮廓/tint 表原样保留。
`research\gaugemono-regular-webfont-DwJfUwLI.woff2` 与之 SHA256 完全一致，是同一份资产。

## 为什么存在

Slint / fontdb 只吃 `.ttf` / `.otf`，不解析 woff2。中央速度数字必须用此字体才能与原
SVG 截图（`viewBox 0 0 286 286`、`font-size:36`、4× 截图 1144 px）逐像素对齐，
因此在开发机上落地一份 ttf 用于度量与渲染校验。

## 边界（硬约束）

1. **禁止**把 `gaugemono.ttf` 复制到 `slint/assets/`、`public/`、`dist/`、资源文件、
   安装器或任何会随二进制/发布包分发的位置。
2. **禁止**把它提交到 git。仓库 `slint/.gitignore` 已排除 `.local-fonts/`；
   提交前请再确认 `git status` 不含本目录。
3. 字体内部已写入约束标记：`name` 表 nameID 0 与 nameID 10 均为
   `LOCAL CALIBRATION ONLY - PROPRIETARY - DO NOT REDISTRIBUTE`，
   `OS/2.fsType = 4`（Preview & Print 嵌入权限）。若在产物里发现该字符串，说明边界被破坏。
4. 公开发布构建缺少本目录时，中央数字回退到系统等宽族（Consolas 等），
   宽度偏差约 +42.6%（见 `slint/docs/font-metrics.md` §5）。
   回退是**可接受的降级**，不是把专有字体塞进产物的理由。

## 会不会影响构建

不会。构建流程不得依赖本目录存在；缺少它时 Slint 侧按
`Gauge Mono` 的 advance 比例（0.389648 em / 数字）预留宽度并保持居中即可。
