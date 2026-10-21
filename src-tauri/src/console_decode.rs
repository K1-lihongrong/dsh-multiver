//! 子进程输出解码（D16）。
//!
//! 背景：中文 Windows 的控制台代码页是 936（GBK），pnpm / PowerShell / node 等
//! 子进程输出的中文按 UTF-8 解码会变成替换字符（U+FFFD），错误信息完全丢失。
//!
//! 策略：先按 UTF-8 **严格**解码（纯 ASCII 与已是 UTF-8 的输出走这条路，零开销）；
//! 失败再按 GBK 解码。GBK 覆盖简体中文 Windows 的绝大多数场景。

/// 把子进程输出的原始字节解码为可读字符串。
///
/// - 先尝试 UTF-8 严格解码（不含替换字符），成功则直接返回
/// - 失败则按 GBK（代码页 936）解码
/// - GBK 也失败时（极罕见）退回 UTF-8 有损解码，保证不 panic
pub fn decode_console(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (cow, _, had_errors) = encoding_rs::GBK.decode(bytes);
            if had_errors {
                // GBK 也有无法映射的字节：退回有损 UTF-8，保留可读部分
                String::from_utf8_lossy(bytes).to_string()
            } else {
                cow.into_owned()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_ascii_passes_through() {
        assert_eq!(decode_console(b"ERR_PNPM_FETCH_404"), "ERR_PNPM_FETCH_404");
    }

    #[test]
    fn utf8_chinese_decodes_as_utf8() {
        let s = "安装失败";
        assert_eq!(decode_console(s.as_bytes()), "安装失败");
    }

    #[test]
    fn gbk_chinese_decodes_as_gbk() {
        // "安装失败" 的 GBK 字节：B0 B2 D7 B0 CA A7 B0 DC
        let gbk: &[u8] = &[0xB0, 0xB2, 0xD7, 0xB0, 0xCA, 0xA7, 0xB0, 0xDC];
        assert_eq!(decode_console(gbk), "安装失败");
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(decode_console(b""), "");
    }

    #[test]
    fn mixed_ascii_and_gbk() {
        // "ERR " + "安装失败" 的 GBK
        let mut bytes = b"ERR ".to_vec();
        bytes.extend_from_slice(&[0xB0, 0xB2, 0xD7, 0xB0, 0xCA, 0xA7, 0xB0, 0xDC]);
        assert_eq!(decode_console(&bytes), "ERR 安装失败");
    }

    #[test]
    fn never_panics_on_arbitrary_bytes() {
        // 任意字节序列都不应 panic
        let garbage: &[u8] = &[0xFF, 0xFE, 0x80, 0x81, 0x00, 0x41];
        let _ = decode_console(garbage);
    }
}
