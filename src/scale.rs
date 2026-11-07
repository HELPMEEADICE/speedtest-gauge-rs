//! 值 ↔ 角度映射与读数格式化。
//!
//! 逐行等价移植 `src/gauge/scale.ts`（全文 85 行）。
//!
//! speedtest.net **不是**线性刻度：刻度值等角分布（270° / (n-1)），指针在所处档位内
//! 做线性插值 —— 所以 `0..100 Mbps` 与 `250..1000 Mbps` 各占 135°。

use crate::geometry::GEO;

/// `GAUGE_LABEL_VALUES`（`scale.ts:26`）：标准 9 档。
pub const LABELS_STANDARD: [f64; 9] = [0.0, 5.0, 10.0, 50.0, 100.0, 250.0, 500.0, 750.0, 1000.0];

/// 扩展 10 档（`scale.ts:34`）：仅当采样超过 1000 Mbps 且 `scaleMode == auto` 时启用。
///
/// 10 档 ⇒ 步长 `270/9 = 30°`，与标准 9 档的 `33.75°` 不同（刻度间距因此保持相近）。
pub const LABELS_EXTENDED: [f64; 10] = [
    0.0, 10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0,
];

/// 无有效读数时显示的字符：`U+2014 EM DASH`（`scale.ts:78`）。
pub const NO_VALUE: &str = "\u{2014}";

/// 第 `index` 个刻度的角度（`scale.ts:36-39`）。
///
/// `count < 2` 时退回起始角；否则 `index / (count - 1) * 270 + (-135)`。
pub fn label_angle_at(index: usize, count: usize) -> f64 {
    if count < 2 {
        return GEO.start_angle;
    }
    (index as f64 / (count - 1) as f64) * (GEO.end_angle - GEO.start_angle) + GEO.start_angle
}

/// Mbps → 角度（`scale.ts:42-57` 的 `angleForValue`）。
///
/// 逐条复刻：
/// - `n == 0` → 起始角；
/// - `!isFinite(mbps) || mbps <= labels[0]` → 第 0 档角（`labels[0]` 恒为 0，故 `mbps <= 0`
///   全走这里）；
/// - `findIndex(x => x >= mbps)`：**第一个** `>=` 的下标；找不到（超上限）→ 末档角（夹紧）；
/// - 命中刻度 → 精确角；否则在 `[labels[idx-1], labels[idx]]` 内线性插值；
/// - `span == 0` 保护（表内无重复值，属防御性写法）。
pub fn angle_for_value(labels: &[f64], mbps: f64) -> f64 {
    let n = labels.len();
    if n == 0 {
        return GEO.start_angle;
    }
    if !mbps.is_finite() || mbps <= labels[0] {
        return label_angle_at(0, n);
    }

    let idx = match labels.iter().position(|x| *x >= mbps) {
        Some(i) => i,
        None => return label_angle_at(n - 1, n),
    };
    if mbps == labels[idx] {
        return label_angle_at(idx, n);
    }

    let i0 = if idx == 0 { 0 } else { idx - 1 };
    let lo = labels[i0];
    let hi = labels[idx];
    let span = hi - lo;
    let t = if span == 0.0 { 0.0 } else { (mbps - lo) / span };
    label_angle_at(i0, n) + t * (label_angle_at(idx, n) - label_angle_at(i0, n))
}

/// 角度 → Mbps（`scale.ts:60-68` 的 `valueForAngle`），[`angle_for_value`] 的反函数。
///
/// 它假设档位等角（由 [`label_angle_at`] 保证），所以用 `step` 反解而不再查表。
/// 源码里只有导出、无内部调用点，属对外 API / 测试用反函数。
///
/// `angle` 为 `NaN` 时返回 `NaN`（JS 的 `Math.min/Math.max` 会传播 NaN）。
pub fn value_for_angle(labels: &[f64], angle: f64) -> f64 {
    let n = labels.len();
    if n < 2 {
        return 0.0;
    }
    if angle.is_nan() {
        return f64::NAN;
    }
    let clamped = angle.clamp(GEO.start_angle, GEO.end_angle);
    let step = (GEO.end_angle - GEO.start_angle) / (n - 1) as f64;
    let idx = ((clamped - GEO.start_angle) / step)
        .floor()
        .clamp(0.0, (n - 2) as f64) as usize;
    let t = (clamped - label_angle_at(idx, n)) / step;
    labels[idx] + t * (labels[idx + 1] - labels[idx])
}

/// kbps → 显示文本（`scale.ts:77-81` 的 `formatSpeed`）。
///
/// - 非有限或负数 → [`NO_VALUE`]（`U+2014`）；
/// - `0` → `"0.00"`（`-0.0` 同样命中）；
/// - 否则 `kbps / 1000` 按 `en-US` 千分位 + **恒定两位小数** 输出。
///
/// **舍入口径（重要）**：`Intl.NumberFormat` 的 `halfExpand` 作用在 double 的
/// **最短往返十进制表示**上（ICU `DecimalQuantity` 的行为），而不是精确展开 —— 与
/// `toFixed`（见 [`kbps_to_mbps`]）不同。实测证据（node v26 / V8）：
///
/// | Mbps | 最短表示 | 精确展开 | `Intl`（本函数） | `toFixed(2)` |
/// |---|---|---|---|---|
/// | 2.675 | `2.675` | `2.67499999999999982…` | **`2.68`** | `2.67` |
/// | 1.005 | `1.005` | `1.00499999999999989…` | **`1.01`** | `1.00` |
/// | 999.995 | `999.995` | `999.995000000000004…` | `1,000.00` | `1000.00` |
pub fn format_speed(kbps: f64) -> String {
    if !kbps.is_finite() || kbps < 0.0 {
        return NO_VALUE.to_string();
    }
    if kbps == 0.0 {
        return "0.00".to_string();
    }
    group_2dp(kbps / 1000.0)
}

/// `parseFloat((kbps / 1000).toFixed(2))`（`scale.ts:83-85`）的等价物。
///
/// `toFixed` 的舍入作用在 double 的**精确十进制展开**上（平局取较大的 n），与 [`format_speed`]
/// 的最短表示口径不同：`kbps = 2675` 时 `format_speed` 给 `"2.68"`，本函数给 `2.67`。
/// 这正是坑：读数显示与指针角度用的是两条不同的量化路径。
pub fn kbps_to_mbps(kbps: f64) -> f64 {
    let mbps = kbps / 1000.0;
    if !mbps.is_finite() {
        // JS: (NaN).toFixed(2) === "NaN" -> parseFloat -> NaN；(Infinity) 同理。
        return mbps;
    }
    let (int_text, frac_text) = round_decimal_2dp(&format!("{:.30}", mbps));
    format!("{}.{}", int_text, frac_text).parse().unwrap_or(0.0)
}

/// 把十进制字面量（不含指数、可带前导 `-`）按 half-up 舍入到两位小数，
/// 返回 `(整数段, 两位小数段)`。
///
/// 平局规则复刻 `toFixed`：`n / 100` 距离原值相等时取**较大**的 `n`，即正数进位、负数向零。
/// 调用方必须把 `text` 准备成"能完整表示原值的十进制串"：
/// - [`format_speed`] 传最短往返表示（`format!("{}", v)`，Rust 的 `Display` 从不使用指数）；
/// - [`kbps_to_mbps`] 传精确展开（`format!("{:.30}", v)`，30 位足够判定第 3 位起的进位）。
fn round_decimal_2dp(text: &str) -> (String, String) {
    let negative = text.starts_with('-');
    let body = if negative { &text[1..] } else { text };
    let (int_part, frac_part) = body.split_once('.').unwrap_or((body, ""));
    let keep = int_part.len() + 2;

    let mut digits: Vec<u8> = Vec::with_capacity(keep + 1);
    digits.extend(int_part.bytes());
    digits.extend(frac_part.bytes());
    if digits.len() > keep {
        let round_up = match digits[keep] {
            b'0'..=b'4' => false,
            b'6'..=b'9' => true,
            // 恰好 5：看后面是否还有非零位；精确平局时正数进位、负数向零（toFixed 规则）。
            _ => digits[keep + 1..].iter().any(|d| *d != b'0') || !negative,
        };
        digits.truncate(keep);
        if round_up {
            let mut i = digits.len();
            loop {
                if i == 0 {
                    digits.insert(0, b'1');
                    break;
                }
                i -= 1;
                if digits[i] == b'9' {
                    digits[i] = b'0';
                } else {
                    digits[i] += 1;
                    break;
                }
            }
        }
    }
    while digits.len() < keep {
        digits.push(b'0');
    }

    let text = String::from_utf8_lossy(&digits).into_owned();
    let split = text.len() - 2;
    let int_text = if negative {
        format!("-{}", &text[..split])
    } else {
        text[..split].to_string()
    };
    (int_text, text[split..].to_string())
}

/// 千分位 + 恒定两位小数（`en-US` 口径）。
fn group_2dp(v: f64) -> String {
    let (int_text, frac_text) = round_decimal_2dp(&format!("{}", v));
    let (negative, digits) = match int_text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, int_text.as_str()),
    };
    let mut out = String::with_capacity(digits.len() + 4 + digits.len() / 3);
    if negative {
        out.push('-');
    }
    let n = digits.len();
    for (i, ch) in digits.char_indices() {
        if i > 0 && (n - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.push('.');
    out.push_str(&frac_text);
    out
}
