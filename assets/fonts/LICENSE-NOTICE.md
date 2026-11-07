# 字体资产许可说明 / Font Asset Licensing

本目录（`slint/assets/fonts/`）随构建产物分发，只包含**许可允许再分发**的字体。

## Montserrat — 可分发

| 项 | 值 |
| --- | --- |
| 文件 | `montserrat.ttf`（176 404 B） |
| 字族 / 字重 | `Montserrat` / `Bold`（`usWeightClass = 700` 静态实例） |
| 许可 | **SIL Open Font License 1.1**，全文见同目录 `OFL.txt` |
| 版权 | Copyright 2011 The Montserrat Project Authors (https://github.com/JulietaUla/Montserrat) |
| 来源 | `C:\oss\Project\speedtest\public\fonts\montserrat.woff2`（可变字体 wght 100–900），
  用 fontTools `instantiateVariableFont(..., {"wght": 700})` 生成静态 Bold 实例 |

OFL 义务（分发时必须保持）：随附 `OFL.txt`；不得单独售卖字体本身；改名后的衍生字体不得使用
保留字体名（Reserved Font Name）。本文件为**未改名**的原始字体实例，保留名称声明照原样生效。

## Gauge Mono — 不可分发

`gaugemono` 是 Ookla 专有仪表字体（Proprietary）。它**不在本目录**，只在
`slint/.local-fonts/gaugemono.ttf` 用于本地像素校准，该目录被 `.gitignore` 排除，
不得进入发布包、安装器或任何可分发产物。详见 `slint/.local-fonts/README.md`。

## 度量

全部实测数值（字宽、墨迹框、em 比例、Slint `font-size` 换算与纵向补偿）见
`slint/docs/font-metrics.md`。
