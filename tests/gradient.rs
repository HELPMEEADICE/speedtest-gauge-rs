//! `gradient` / `gradient_data` 模块的等价性测试。
//!
//! 除了硬编码的黄金值，这里还会**扫 `docs/gradient-stops.json`** 与生成出来的
//! `gradient_data.rs` 逐条比对，确保数据链路 `gradient-stops.ts -> json -> .rs` 没有漂移。

use speed_gauge_slint::gradient::{
    ring_segment_color, ring_segments, ring_segments_default, sample_stops, segment_angles, stops,
    DEFAULT_SEGMENTS, OVERLAP_DEG,
};
use speed_gauge_slint::model::GradientVariant;

const VARIANTS: [GradientVariant; 4] = [
    GradientVariant::DlMulti,
    GradientVariant::UlMulti,
    GradientVariant::DlSingle,
    GradientVariant::UlSingle,
];

fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let body = text.strip_prefix('#')?;
    if body.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&body[0..2], 16).ok()?;
    let g = u8::from_str_radix(&body[2..4], 16).ok()?;
    let b = u8::from_str_radix(&body[4..6], 16).ok()?;
    Some([r, g, b])
}

type StopList = Vec<(f64, [u8; 3])>;
type JsonGroup = (String, StopList);

/// 极简手写解析器：只认 `"dl-multi": [` 这类分组头与 `{ "t": 0.0370, "hex": "#0bbdfd" }`。
fn parse_json_groups(text: &str) -> Vec<JsonGroup> {
    let mut groups: Vec<JsonGroup> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('"') && line.ends_with('[') {
            let name = line
                .trim_start_matches('"')
                .trim_end_matches(['"', ':', ' ', '['])
                .to_string();
            groups.push((name, Vec::new()));
            continue;
        }
        if line.starts_with(']') {
            continue;
        }
        if !line.starts_with('{') {
            continue;
        }
        let t_pos = match line.find("\"t\":") {
            Some(p) => p + 4,
            None => continue,
        };
        let rest = &line[t_pos..];
        let comma = match rest.find(',') {
            Some(c) => c,
            None => continue,
        };
        let t: f64 = match rest[..comma].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let hex_pos = match line.find("\"hex\":") {
            Some(p) => p + 6,
            None => continue,
        };
        let rest = &line[hex_pos..];
        let open = match rest.find('"') {
            Some(p) => p + 1,
            None => continue,
        };
        let rest = &rest[open..];
        let close = match rest.find('"') {
            Some(p) => p,
            None => continue,
        };
        let color = match parse_hex(&rest[..close]) {
            Some(c) => c,
            None => continue,
        };
        if let Some((_, entries)) = groups.last_mut() {
            entries.push((t, color));
        }
    }
    groups
}

#[test]
fn four_variants_have_28_stops_each() {
    for variant in VARIANTS {
        let table = stops(variant);
        assert_eq!(table.len(), 28, "{variant} 的 stop 数量变了");
        assert_eq!(table[0].0, 0.0);
        assert_eq!(table[27].0, 1.0);
        // t 单调递增
        for pair in table.windows(2) {
            assert!(pair[1].0 > pair[0].0, "{variant} 的 t 非单调");
        }
    }
}

#[test]
fn first_and_last_colors_verbatim() {
    // 首尾色（末两项正是附录 B1/B2 里 README 与源码不一致的地方，以源码为准）。
    assert_eq!(stops(GradientVariant::DlMulti)[0].1, [0x00, 0xb5, 0xfe]);
    assert_eq!(stops(GradientVariant::DlMulti)[27].1, [0x7b, 0xfe, 0x71]);
    assert_eq!(stops(GradientVariant::UlMulti)[0].1, [0x75, 0x53, 0xd0]);
    assert_eq!(stops(GradientVariant::UlMulti)[27].1, [0xfe, 0x70, 0xa8]);
    // dl-single 终点是 #ffb96b（README 误写 #ffba6b，见 B1）
    assert_eq!(stops(GradientVariant::DlSingle)[0].1, [0xff, 0x6b, 0x7b]);
    assert_eq!(stops(GradientVariant::DlSingle)[27].1, [0xff, 0xb9, 0x6b]);
    assert_eq!(stops(GradientVariant::DlSingle)[20].1, [0xff, 0xba, 0x6b]);
    // ul-single 终点是 #ceff70（README 误写 #cfff70，见 B2）
    assert_eq!(stops(GradientVariant::UlSingle)[0].1, [0x70, 0xff, 0x9f]);
    assert_eq!(stops(GradientVariant::UlSingle)[27].1, [0xce, 0xff, 0x70]);
    assert_eq!(stops(GradientVariant::UlSingle)[20].1, [0xcf, 0xff, 0x70]);
}

/// 数据链路：`docs/gradient-stops.json` 与生成的 `src/gradient_data.rs` 必须逐条一致。
#[test]
fn generated_data_matches_docs_json() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/gradient-stops.json");
    let text = std::fs::read_to_string(path).expect("读不到 docs/gradient-stops.json");
    let groups = parse_json_groups(&text);
    assert_eq!(groups.len(), 4, "分组数量不对: {:?}", groups.len());

    let mut total = 0usize;
    for (name, entries) in &groups {
        let variant =
            GradientVariant::parse(name).unwrap_or_else(|| panic!("json 里出现未知变体 {name}"));
        let table = stops(variant);
        assert_eq!(entries.len(), 28, "{name} 条数不对");
        for (i, (t, color)) in entries.iter().enumerate() {
            assert_eq!(table[i].0, *t, "{name} 第 {i} 条 t 不一致");
            assert_eq!(table[i].1, *color, "{name} 第 {i} 条颜色不一致");
            total += 1;
        }
    }
    assert_eq!(total, 112);
}

#[test]
fn sample_stops_endpoints_and_interpolation() {
    let table: [(f64, [u8; 3]); 2] = [(0.0, [0, 0, 0]), (1.0, [255, 255, 255])];
    // 127.5 -> Math.round -> 128
    assert_eq!(sample_stops(&table, 0.5), [128, 128, 128]);
    // 63.75 -> 64
    assert_eq!(sample_stops(&table, 0.25), [64, 64, 64]);
    assert_eq!(sample_stops(&table, 0.0), [0, 0, 0]);
    assert_eq!(sample_stops(&table, 1.0), [255, 255, 255]);
    // clamp(0, 1)
    assert_eq!(sample_stops(&table, -3.0), [0, 0, 0]);
    assert_eq!(sample_stops(&table, 7.0), [255, 255, 255]);
    // NaN 不会命中任何区间 ⇒ 兜底返回末色
    assert_eq!(sample_stops(&table, f64::NAN), [255, 255, 255]);
    // span == 0 的保护分支
    let dup: [(f64, [u8; 3]); 2] = [(0.5, [1, 2, 3]), (0.5, [9, 9, 9])];
    assert_eq!(sample_stops(&dup, 0.5), [1, 2, 3]);
    // 空表 / 单条表
    assert_eq!(sample_stops(&[], 0.3), [0, 0, 0]);
    assert_eq!(sample_stops(&[(0.0, [4, 5, 6])], 0.3), [4, 5, 6]);

    // 真实表的端点
    let dl = stops(GradientVariant::DlMulti);
    assert_eq!(sample_stops(dl, 0.0), dl[0].1);
    assert_eq!(sample_stops(dl, 1.0), dl[27].1);
    // 命中 stop 边界时先命中的是左边那一对，k = 1 ⇒ 等于该 stop 本身
    assert_eq!(sample_stops(dl, 0.0741), dl[2].1);
    assert_eq!(sample_stops(dl, 0.5 * (dl[0].0 + dl[1].0)), {
        let k = 0.5;
        let a = dl[0].1;
        let b = dl[1].1;
        let mut out = [0u8; 3];
        for ch in 0..3 {
            let from = a[ch] as f64;
            let to = b[ch] as f64;
            out[ch] = (from + (to - from) * k + 0.5).floor() as u8;
        }
        out
    });
}

#[test]
fn ring_segments_goldens_and_count() {
    let segs = ring_segments_default(GradientVariant::DlMulti);
    assert_eq!(segs.len(), DEFAULT_SEGMENTS);
    assert_eq!(DEFAULT_SEGMENTS, 240);
    assert_eq!(OVERLAP_DEG, 0.28);

    // 第 0 段：a0 = -135，a1 = -133.595（含 +0.28 overlap）
    assert_eq!(segs[0].d, "M 51.076 234.924 A 130 130 0 0 1 48.85 232.642");
    // 第 239 段：a1 被 min(135, ...) 截断 ⇒ 无明显 overlap（附录 B7，源码真实行为）
    assert_eq!(
        segs[239].d,
        "M 236.711 233.101 A 130 130 0 0 1 234.924 234.924"
    );
    // 首末色不同（渐变的两端）
    assert_ne!(segs[0].color, segs[239].color);
    // 每段都是合法路径串
    for seg in &segs {
        assert!(seg.d.starts_with("M "), "非法路径: {}", seg.d);
        assert!(seg.d.contains(" A 130 130 0 "), "半径不对: {}", seg.d);
    }
    // 240 段路径字符串两两不同（除首末段角度外，角度互不相同）
    let mut unique: Vec<&str> = segs.iter().map(|s| s.d.as_str()).collect();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), DEFAULT_SEGMENTS);
}

#[test]
fn ring_segment_colors_stay_in_table_gamut() {
    for variant in VARIANTS {
        let table = stops(variant);
        let mut lo = [255u8; 3];
        let mut hi = [0u8; 3];
        for (_, color) in table {
            for ch in 0..3 {
                lo[ch] = lo[ch].min(color[ch]);
                hi[ch] = hi[ch].max(color[ch]);
            }
        }
        for i in 0..DEFAULT_SEGMENTS {
            let color = ring_segment_color(variant, i, DEFAULT_SEGMENTS);
            for ch in 0..3 {
                assert!(
                    color[ch] >= lo[ch] && color[ch] <= hi[ch],
                    "{variant} 第 {i} 段通道 {ch} 越界: {} 不在 [{}, {}]",
                    color[ch],
                    lo[ch],
                    hi[ch]
                );
            }
        }
    }
}

#[test]
fn ring_segment_color_uses_segment_centre_sampling() {
    // (i + 0.5) / n：与直接取第 i 条 stop 不同。
    let table = stops(GradientVariant::DlMulti);
    let n = DEFAULT_SEGMENTS;
    for i in [0usize, 1, 119, 238, 239] {
        let expected = sample_stops(table, (i as f64 + 0.5) / n as f64);
        assert_eq!(ring_segment_color(GradientVariant::DlMulti, i, n), expected);
    }
    // n == 0 的防御分支
    assert_eq!(
        ring_segment_color(GradientVariant::DlMulti, 0, 0),
        [0, 0, 0]
    );
    assert!(ring_segments(GradientVariant::DlMulti, 0, 130.0, 143.0, 143.0).is_empty());
}

#[test]
fn segment_angles_golden_and_last_segment_has_no_overlap() {
    // n = 240：step = 270/240 = 1.125，overlap = 0.28
    let (a0, a1) = segment_angles(0, 240);
    assert_eq!(a0, -135.0);
    assert!((a1 - -133.595).abs() < 1e-12, "首段 a1 = {a1}");
    assert!((a1 - a0 - 1.405).abs() < 1e-12, "首段角宽 = {}", a1 - a0);

    let (a0, a1) = segment_angles(239, 240);
    assert!((a0 - 133.875).abs() < 1e-12, "末段 a0 = {a0}");
    assert_eq!(a1, 135.0);
    // 末段被 min 截断 ⇒ 角宽只有 1.125（附录 B7）
    assert!((a1 - a0 - 1.125).abs() < 1e-12, "末段角宽 = {}", a1 - a0);

    // 中间段保持 1.405 的角宽
    let (mid0, mid1) = segment_angles(120, 240);
    assert!((mid1 - mid0 - 1.405).abs() < 1e-12);

    // 防御分支
    assert_eq!(segment_angles(0, 0), (-135.0, -135.0));
}

#[test]
fn ring_segments_are_deterministic_and_variant_specific() {
    let a = ring_segments(GradientVariant::UlSingle, 12, 130.0, 143.0, 143.0);
    let b = ring_segments(GradientVariant::UlSingle, 12, 130.0, 143.0, 143.0);
    assert_eq!(a, b);
    assert_eq!(a.len(), 12);
    // 不同变体同一段颜色不同
    let other = ring_segments(GradientVariant::DlMulti, 12, 130.0, 143.0, 143.0);
    assert_ne!(a[0].color, other[0].color);
    // n=12 时第 0 段 a1 = -135 + 270/12 + 0.28 = -112.22
    assert_eq!(a[0].d, "M 51.076 234.924 A 130 130 0 0 1 22.654 192.161");
}
