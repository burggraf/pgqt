//! COPY encoding support
//!
//! This module handles character encoding conversion for COPY operations.
//! Uses encoding_rs for high-performance encoding/decoding.

use encoding_rs::{
    Encoding, UTF_8, WINDOWS_1250, WINDOWS_1251, WINDOWS_1252, WINDOWS_1253, WINDOWS_1254,
    WINDOWS_1255, WINDOWS_1256, WINDOWS_1257, WINDOWS_1258, EUC_JP, SHIFT_JIS, EUC_KR, GB18030,
    BIG5, KOI8_R, KOI8_U, ISO_8859_2, ISO_8859_3, ISO_8859_4, ISO_8859_5, ISO_8859_6,
    ISO_8859_7, ISO_8859_8, ISO_8859_10, ISO_8859_13, ISO_8859_14, ISO_8859_15, ISO_8859_16,
};
use anyhow::{anyhow, Result};

/// Supported encodings for COPY operations
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum CopyEncoding {
    Utf8,
    Latin1,
    Latin2,
    Latin3,
    Latin4,
    Latin5,
    Latin6,
    Latin7,
    Latin8,
    Latin9,
    Latin10,
    Windows1250,
    Windows1251,
    Windows1252,
    Windows1253,
    Windows1254,
    Windows1255,
    Windows1256,
    Windows1257,
    Windows1258,
    EucJp,
    ShiftJis,
    EucKr,
    Gb18030,
    Gbk,
    Big5,
    Koi8R,
    Koi8U,
    Iso8859_2,
    Iso8859_3,
    Iso8859_4,
    Iso8859_5,
    Iso8859_6,
    Iso8859_7,
    Iso8859_8,
    Iso8859_10,
    Iso8859_13,
    Iso8859_14,
    Iso8859_15,
    Iso8859_16,
    SqlAscii,
}

impl CopyEncoding {
    /// Parse encoding name from string (case-insensitive)
    pub fn from_name(name: &str) -> Result<Self> {
        let upper = name.to_uppercase();
        let name_clean = upper.replace("-", "").replace("_", "");
        
        match name_clean.as_str() {
            "UTF8" | "UTF" => Ok(CopyEncoding::Utf8),
            "LATIN1" | "ISO88591" => Ok(CopyEncoding::Latin1),
            "LATIN2" | "ISO88592" => Ok(CopyEncoding::Latin2),
            "LATIN3" | "ISO88593" => Ok(CopyEncoding::Latin3),
            "LATIN4" | "ISO88594" => Ok(CopyEncoding::Latin4),
            "LATIN5" | "ISO88599" => Ok(CopyEncoding::Latin5),
            "LATIN6" | "ISO885910" => Ok(CopyEncoding::Latin6),
            "LATIN7" | "ISO885913" => Ok(CopyEncoding::Latin7),
            "LATIN8" | "ISO885914" => Ok(CopyEncoding::Latin8),
            "LATIN9" | "ISO885915" => Ok(CopyEncoding::Latin9),
            "LATIN10" | "ISO885916" => Ok(CopyEncoding::Latin10),
            "WINDOWS1250" | "WIN1250" | "CP1250" => Ok(CopyEncoding::Windows1250),
            "WINDOWS1251" | "WIN1251" | "CP1251" => Ok(CopyEncoding::Windows1251),
            "WINDOWS1252" | "WIN1252" | "CP1252" => Ok(CopyEncoding::Windows1252),
            "WINDOWS1253" | "WIN1253" | "CP1253" => Ok(CopyEncoding::Windows1253),
            "WINDOWS1254" | "WIN1254" | "CP1254" => Ok(CopyEncoding::Windows1254),
            "WINDOWS1255" | "WIN1255" | "CP1255" => Ok(CopyEncoding::Windows1255),
            "WINDOWS1256" | "WIN1256" | "CP1256" => Ok(CopyEncoding::Windows1256),
            "WINDOWS1257" | "WIN1257" | "CP1257" => Ok(CopyEncoding::Windows1257),
            "WINDOWS1258" | "WIN1258" | "CP1258" => Ok(CopyEncoding::Windows1258),
            "EUCJP" | "EUCCPJP" => Ok(CopyEncoding::EucJp),
            "SHIFTJIS" | "SJIS" | "MSKANJI" | "SHIFTJS" => Ok(CopyEncoding::ShiftJis),
            "EUCKR" | "EUCCPKR" | "KSC5601" => Ok(CopyEncoding::EucKr),
            "GB18030" => Ok(CopyEncoding::Gb18030),
            "GBK" | "CP936" => Ok(CopyEncoding::Gbk),
            "BIG5" | "BIG5HKSCS" | "BIG5HKSCS2001" | "BIG5HKSCS1999" => Ok(CopyEncoding::Big5),
            "KOI8R" => Ok(CopyEncoding::Koi8R),
            "KOI8U" => Ok(CopyEncoding::Koi8U),
            "SQLASCII" => Ok(CopyEncoding::SqlAscii),
            _ => Err(anyhow!("Unsupported encoding: {}", name)),
        }
    }

    /// Get the display name for this encoding
    pub fn to_name(&self) -> &'static str {
        match self {
            CopyEncoding::Utf8 => "UTF8",
            CopyEncoding::Latin1 => "LATIN1",
            CopyEncoding::Latin2 => "LATIN2",
            CopyEncoding::Latin3 => "LATIN3",
            CopyEncoding::Latin4 => "LATIN4",
            CopyEncoding::Latin5 => "LATIN5",
            CopyEncoding::Latin6 => "LATIN6",
            CopyEncoding::Latin7 => "LATIN7",
            CopyEncoding::Latin8 => "LATIN8",
            CopyEncoding::Latin9 => "LATIN9",
            CopyEncoding::Latin10 => "LATIN10",
            CopyEncoding::Windows1250 => "WINDOWS-1250",
            CopyEncoding::Windows1251 => "WINDOWS-1251",
            CopyEncoding::Windows1252 => "WINDOWS-1252",
            CopyEncoding::Windows1253 => "WINDOWS-1253",
            CopyEncoding::Windows1254 => "WINDOWS-1254",
            CopyEncoding::Windows1255 => "WINDOWS-1255",
            CopyEncoding::Windows1256 => "WINDOWS-1256",
            CopyEncoding::Windows1257 => "WINDOWS-1257",
            CopyEncoding::Windows1258 => "WINDOWS-1258",
            CopyEncoding::EucJp => "EUC_JP",
            CopyEncoding::ShiftJis => "SHIFT_JIS",
            CopyEncoding::EucKr => "EUC_KR",
            CopyEncoding::Gb18030 => "GB18030",
            CopyEncoding::Gbk => "GBK",
            CopyEncoding::Big5 => "BIG5",
            CopyEncoding::Koi8R => "KOI8-R",
            CopyEncoding::Koi8U => "KOI8-U",
            CopyEncoding::Iso8859_2 => "ISO-8859-2",
            CopyEncoding::Iso8859_3 => "ISO-8859-3",
            CopyEncoding::Iso8859_4 => "ISO-8859-4",
            CopyEncoding::Iso8859_5 => "ISO-8859-5",
            CopyEncoding::Iso8859_6 => "ISO-8859-6",
            CopyEncoding::Iso8859_7 => "ISO-8859-7",
            CopyEncoding::Iso8859_8 => "ISO-8859-8",
            CopyEncoding::Iso8859_10 => "ISO-8859-10",
            CopyEncoding::Iso8859_13 => "ISO-8859-13",
            CopyEncoding::Iso8859_14 => "ISO-8859-14",
            CopyEncoding::Iso8859_15 => "ISO-8859-15",
            CopyEncoding::Iso8859_16 => "ISO-8859-16",
            CopyEncoding::SqlAscii => "SQL_ASCII",
        }
    }

    /// Get the encoding_rs Encoding for this encoding
    fn to_encoding_rs(&self) -> Option<&'static Encoding> {
        match self {
            CopyEncoding::Utf8 => Some(UTF_8),
            // LATIN1 is ISO-8859-1 which is not directly in encoding_rs
            // Use WINDOWS-1252 as it's a superset of ISO-8859-1
            CopyEncoding::Latin1 => Some(WINDOWS_1252),
            CopyEncoding::Latin2 => Some(ISO_8859_2),
            CopyEncoding::Latin3 => Some(ISO_8859_3),
            CopyEncoding::Latin4 => Some(ISO_8859_4),
            // LATIN5 is ISO-8859-9 (Turkish), not in encoding_rs
            CopyEncoding::Latin5 => None,
            CopyEncoding::Latin6 => Some(ISO_8859_10),
            CopyEncoding::Latin7 => Some(ISO_8859_13),
            CopyEncoding::Latin8 => Some(ISO_8859_14),
            CopyEncoding::Latin9 => Some(ISO_8859_15),
            CopyEncoding::Latin10 => Some(ISO_8859_16),
            CopyEncoding::Windows1250 => Some(WINDOWS_1250),
            CopyEncoding::Windows1251 => Some(WINDOWS_1251),
            CopyEncoding::Windows1252 => Some(WINDOWS_1252),
            CopyEncoding::Windows1253 => Some(WINDOWS_1253),
            CopyEncoding::Windows1254 => Some(WINDOWS_1254),
            CopyEncoding::Windows1255 => Some(WINDOWS_1255),
            CopyEncoding::Windows1256 => Some(WINDOWS_1256),
            CopyEncoding::Windows1257 => Some(WINDOWS_1257),
            CopyEncoding::Windows1258 => Some(WINDOWS_1258),
            CopyEncoding::EucJp => Some(EUC_JP),
            CopyEncoding::ShiftJis => Some(SHIFT_JIS),
            CopyEncoding::EucKr => Some(EUC_KR),
            CopyEncoding::Gb18030 => Some(GB18030),
            CopyEncoding::Gbk => Some(GB18030), // GB18030 is a superset of GBK
            CopyEncoding::Big5 => Some(BIG5),
            CopyEncoding::Koi8R => Some(KOI8_R),
            CopyEncoding::Koi8U => Some(KOI8_U),
            CopyEncoding::Iso8859_2 => Some(ISO_8859_2),
            CopyEncoding::Iso8859_3 => Some(ISO_8859_3),
            CopyEncoding::Iso8859_4 => Some(ISO_8859_4),
            CopyEncoding::Iso8859_5 => Some(ISO_8859_5),
            CopyEncoding::Iso8859_6 => Some(ISO_8859_6),
            CopyEncoding::Iso8859_7 => Some(ISO_8859_7),
            CopyEncoding::Iso8859_8 => Some(ISO_8859_8),
            CopyEncoding::Iso8859_10 => Some(ISO_8859_10),
            CopyEncoding::Iso8859_13 => Some(ISO_8859_13),
            CopyEncoding::Iso8859_14 => Some(ISO_8859_14),
            CopyEncoding::Iso8859_15 => Some(ISO_8859_15),
            CopyEncoding::Iso8859_16 => Some(ISO_8859_16),
            CopyEncoding::SqlAscii => None, // SQL_ASCII treats bytes as-is
        }
    }

    /// Check if this is UTF-8 (no conversion needed)
    pub fn is_utf8(&self) -> bool {
        matches!(self, CopyEncoding::Utf8)
    }

    /// Check if this is SQL_ASCII (no conversion, treats bytes as-is)
    pub fn is_sql_ascii(&self) -> bool {
        matches!(self, CopyEncoding::SqlAscii)
    }
}

/// Convert bytes from source encoding to UTF-8
/// 
/// For UTF-8 source, returns the bytes as a string without conversion.
/// For other encodings, decodes to UTF-8.
/// SQL_ASCII treats all bytes as valid and decodes as Latin1 (bytes 0x00-0x7F are ASCII,
/// 0x80-0xFF pass through as-is in the resulting string).
pub fn decode_to_utf8(bytes: &[u8], encoding: &CopyEncoding) -> Result<String> {
    // Fast path: UTF-8 needs no conversion
    if encoding.is_utf8() {
        return Ok(String::from_utf8_lossy(bytes).to_string());
    }

    // SQL_ASCII: treat bytes as-is (map high bytes to Unicode private use)
    if encoding.is_sql_ascii() {
        // For SQL_ASCII, we treat each byte as a character
        // High bytes (0x80-0xFF) are mapped to Unicode code points U+0080-U+00FF
        let decoded: String = bytes.iter().map(|&b| b as char).collect();
        return Ok(decoded);
    }

    // Special case for LATIN1 (ISO-8859-1): direct byte-to-char mapping
    if matches!(encoding, CopyEncoding::Latin1) {
        let decoded: String = bytes.iter().map(|&b| b as char).collect();
        return Ok(decoded);
    }

    // Get the encoding_rs encoding
    let enc = encoding.to_encoding_rs()
        .ok_or_else(|| anyhow!("Encoding {} not supported", encoding.to_name()))?;

    // Decode with error handling
    let (cow, _encoding_used, had_errors) = enc.decode(bytes);
    
    if had_errors {
        return Err(anyhow!(
            "Invalid byte sequence for encoding {}",
            encoding.to_name()
        ));
    }
    
    Ok(cow.into_owned())
}

/// Convert UTF-8 string to target encoding
///
/// For UTF-8 target, returns the string bytes directly.
/// For other encodings, encodes from UTF-8.
/// SQL_ASCII encodes by taking the low byte of each character.
pub fn encode_from_utf8(text: &str, encoding: &CopyEncoding) -> Result<Vec<u8>> {
    // Fast path: UTF-8 needs no conversion
    if encoding.is_utf8() {
        return Ok(text.as_bytes().to_vec());
    }

    // SQL_ASCII: take low byte of each character
    if encoding.is_sql_ascii() {
        let encoded: Vec<u8> = text.chars().map(|c| (c as u32) as u8).collect();
        return Ok(encoded);
    }

    // Special case for LATIN1 (ISO-8859-1): check if all chars are in range 0-255
    if matches!(encoding, CopyEncoding::Latin1) {
        let mut encoded = Vec::with_capacity(text.len());
        for c in text.chars() {
            let code = c as u32;
            if code > 255 {
                return Err(anyhow!(
                    "Character '{}' (U+{:04X}) cannot be encoded in LATIN1",
                    c, code
                ));
            }
            encoded.push(code as u8);
        }
        return Ok(encoded);
    }

    // Get the encoding_rs encoding
    let enc = encoding.to_encoding_rs()
        .ok_or_else(|| anyhow!("Encoding {} not supported", encoding.to_name()))?;

    // Encode with error handling
    let (cow, _encoding_used, had_errors) = enc.encode(text);
    
    if had_errors {
        return Err(anyhow!(
            "Character cannot be encoded in {}",
            encoding.to_name()
        ));
    }
    
    Ok(cow.into_owned())
}

/// Convert bytes from one encoding to another
///
/// This is useful for encoding conversion without going through UTF-8 string,
/// but internally it does decode then encode.
#[allow(dead_code)]
pub fn convert_encoding(bytes: &[u8], from: &CopyEncoding, to: &CopyEncoding) -> Result<Vec<u8>> {
    if from == to {
        return Ok(bytes.to_vec());
    }
    
    let utf8 = decode_to_utf8(bytes, from)?;
    encode_from_utf8(&utf8, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding_from_name() {
        assert_eq!(CopyEncoding::from_name("UTF8").unwrap(), CopyEncoding::Utf8);
        assert_eq!(CopyEncoding::from_name("utf-8").unwrap(), CopyEncoding::Utf8);
        assert_eq!(CopyEncoding::from_name("UTF-8").unwrap(), CopyEncoding::Utf8);
        assert_eq!(CopyEncoding::from_name("LATIN1").unwrap(), CopyEncoding::Latin1);
        assert_eq!(CopyEncoding::from_name("ISO-8859-1").unwrap(), CopyEncoding::Latin1);
        assert_eq!(CopyEncoding::from_name("WINDOWS-1252").unwrap(), CopyEncoding::Windows1252);
        assert_eq!(CopyEncoding::from_name("WIN-1252").unwrap(), CopyEncoding::Windows1252);
        assert_eq!(CopyEncoding::from_name("EUC_JP").unwrap(), CopyEncoding::EucJp);
        assert_eq!(CopyEncoding::from_name("EUC-JP").unwrap(), CopyEncoding::EucJp);
        
        assert!(CopyEncoding::from_name("INVALID").is_err());
        assert!(CopyEncoding::from_name("").is_err());
    }

    #[test]
    fn test_utf8_roundtrip() {
        let original = "Hello, World! 你好世界";
        let encoded = encode_from_utf8(original, &CopyEncoding::Utf8).unwrap();
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::Utf8).unwrap();
        assert_eq!(original, decoded);
        // UTF-8 encoding should be a no-op (just return bytes)
        assert_eq!(encoded, original.as_bytes());
    }

    #[test]
    fn test_latin1_roundtrip() {
        let original = "Hello, Wörld! ÄÖÜ";
        let encoded = encode_from_utf8(original, &CopyEncoding::Latin1).unwrap();
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::Latin1).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_latin1_german() {
        // German characters in LATIN1
        let original = "äöüÄÖÜß";
        let encoded = encode_from_utf8(original, &CopyEncoding::Latin1).unwrap();
        // These should encode to single bytes
        assert_eq!(encoded.len(), 7);
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::Latin1).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_windows1252_euro() {
        // Euro sign is in WINDOWS-1252 but not LATIN1
        let original = "€100";
        let encoded = encode_from_utf8(original, &CopyEncoding::Windows1252).unwrap();
        // Euro is 0x80 in WINDOWS-1252
        assert_eq!(encoded, vec![0x80, b'1', b'0', b'0']);
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::Windows1252).unwrap();
        assert_eq!(original, decoded);
        
        // Should fail for LATIN1 (Euro not representable)
        assert!(encode_from_utf8(original, &CopyEncoding::Latin1).is_err());
    }

    #[test]
    fn test_latin1_all_bytes_valid() {
        // LATIN1 accepts all byte values 0x00-0xFF
        let all_bytes: Vec<u8> = (0..=255).collect();
        let decoded = decode_to_utf8(&all_bytes, &CopyEncoding::Latin1).unwrap();
        // Should succeed - all bytes are valid in LATIN1
        assert!(!decoded.is_empty());
    }

    #[test]
    fn test_euc_jp_roundtrip() {
        // Japanese text
        let original = "日本語テスト";
        let encoded = encode_from_utf8(original, &CopyEncoding::EucJp).unwrap();
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::EucJp).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_shift_jis_roundtrip() {
        // Japanese text in Shift_JIS
        let original = "こんにちは";
        let encoded = encode_from_utf8(original, &CopyEncoding::ShiftJis).unwrap();
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::ShiftJis).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_encoding_names() {
        assert_eq!(CopyEncoding::Utf8.to_name(), "UTF8");
        assert_eq!(CopyEncoding::Latin1.to_name(), "LATIN1");
        assert_eq!(CopyEncoding::Windows1252.to_name(), "WINDOWS-1252");
        assert_eq!(CopyEncoding::EucJp.to_name(), "EUC_JP");
    }

    #[test]
    fn test_is_utf8() {
        assert!(CopyEncoding::Utf8.is_utf8());
        assert!(!CopyEncoding::Latin1.is_utf8());
        assert!(!CopyEncoding::Windows1252.is_utf8());
    }

    #[test]
    fn test_convert_encoding() {
        // Convert from LATIN1 to WINDOWS-1252
        let latin1_text = "café";
        let latin1_bytes = encode_from_utf8(latin1_text, &CopyEncoding::Latin1).unwrap();
        let win1252_bytes = convert_encoding(&latin1_bytes, &CopyEncoding::Latin1, &CopyEncoding::Windows1252).unwrap();
        let decoded = decode_to_utf8(&win1252_bytes, &CopyEncoding::Windows1252).unwrap();
        assert_eq!(latin1_text, decoded);
    }

    #[test]
    fn test_sql_ascii() {
        // SQL_ASCII treats bytes as-is
        let bytes = vec![0x41, 0x80, 0xFF]; // 'A', high byte, max byte
        let decoded = decode_to_utf8(&bytes, &CopyEncoding::SqlAscii).unwrap();
        // Should decode without error - check char count, not byte count
        // (high bytes become multi-byte UTF-8 in the string)
        assert_eq!(decoded.chars().count(), 3);
        
        // Re-encode should give back same bytes
        let encoded = encode_from_utf8(&decoded, &CopyEncoding::SqlAscii).unwrap();
        assert_eq!(encoded, bytes);
    }

    #[test]
    fn test_windows_cyrillic() {
        // Russian text in WINDOWS-1251
        let original = "Привет мир"; // "Hello world" in Russian
        let encoded = encode_from_utf8(original, &CopyEncoding::Windows1251).unwrap();
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::Windows1251).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_korean_euc_kr() {
        // Korean text in EUC-KR
        let original = "안녕하세요";
        let encoded = encode_from_utf8(original, &CopyEncoding::EucKr).unwrap();
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::EucKr).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_chinese_gbk() {
        // Chinese text in GBK
        let original = "你好世界";
        let encoded = encode_from_utf8(original, &CopyEncoding::Gbk).unwrap();
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::Gbk).unwrap();
        assert_eq!(original, decoded);
    }
}
