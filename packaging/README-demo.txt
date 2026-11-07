speedtest-gauge-rs — demo binary
===============================

这是一个自包含的示例程序：字体（Montserrat Bold, SIL OFL 1.1）已经编译进二进制，
不需要任何外部资源文件即可运行。

运行
----
    ./demo              (Linux / macOS)
    demo.exe            (Windows)

窗口里左边是主仪表盘，右边是控制面板，右下角还有一个独立的小仪表（用来演示
多实例互不干扰）。拖动滑块、点 Run Demo 看完整测速流程。

可选：让中央数字用原版字体
--------------------------
中央数字默认使用系统等宽字体，比原版宽约 42%。原版用的 Gauge Mono 是 Ookla 的
专有字体，不随本仓库分发。如果你本机合法持有该字体（文件名 gaugemono.ttf），
把它放到可执行文件同级目录，或用 -- 放到下面的位置，程序会自动加载：

    ./.local-fonts/gaugemono.ttf      (相对可执行文件所在目录)
    ./gaugemono.ttf                    (相对可执行文件所在目录或当前工作目录)

程序启动时会打印一行 "fonts: N face(s): ...；缺失：..." 告诉你到底加载到了什么。

许可
----
本程序代码：WTFPL v2（见随附 LICENSE）。
Montserrat：SIL OFL 1.1（见 https://github.com/HELPMEEADICE/speedtest-gauge-rs
            的 assets/fonts/OFL.txt）。
Slint 运行时：GPLv3 / 商业双许可，使用者自行遵守。

源码：https://github.com/HELPMEEADICE/speedtest-gauge-rs
