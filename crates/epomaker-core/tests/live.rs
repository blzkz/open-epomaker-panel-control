//! Host-streamed effects against vectors recorded from the vendor code
//! (`tools/re-harness/gen_golden_live.js`).

use epomaker_core::lighting::Rgb;
use epomaker_core::live::{encode_music_frame, encode_screen_color, music_levels, MUSIC_MAX_LEVEL};
use serde_json::Value;

const GOLDEN: &str = include_str!("data/golden_live.json");

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn golden() -> Value {
    serde_json::from_str(GOLDEN).unwrap()
}

#[test]
fn frames_match_vendor() {
    let g = golden();
    let mut n = 0;
    for c in g["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let (kind, arg) = name.split_once(' ').unwrap();
        let values: Vec<u8> = serde_json::from_str::<Vec<u8>>(arg).unwrap();
        let frame = match kind {
            "music" => encode_music_frame(&values),
            "screen" => encode_screen_color(Rgb::new(values[0], values[1], values[2])),
            k => panic!("{k}"),
        };
        let w = &c["writes"][0];
        assert_eq!(c["writes"].as_array().unwrap().len(), 1, "{name}");
        assert_eq!(w["ck"].as_u64().unwrap() as u8, frame.checksum.driver_id(), "{name}");
        assert_eq!(frame.wire_bytes().to_vec(), unhex(w["data"].as_str().unwrap()), "{name}");
        n += 1;
    }
    assert_eq!(n, 12);
}

#[test]
fn levels_match_vendor_algorithm() {
    let g = golden();
    for c in g["levels"].as_array().unwrap() {
        let spectrum = unhex(c["spectrum"].as_str().unwrap());
        let want: Vec<u8> = c["levels"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap_or(0) as u8).collect();
        assert_eq!(music_levels(&spectrum, MUSIC_MAX_LEVEL).to_vec(), want);
    }
}
