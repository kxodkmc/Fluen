//! 简单的可逆字符串混淆（防止源码与二进制中出现明文密钥）。
//!
//! # 算法
//!
//! 1. **XOR**：每个字节与 [`XOR_KEY`] 异或
//! 2. **Hex 编码**：转成两位十六进制字符串（保证可打印）
//! 3. **置换**：用固定 seed 的 LCG 生成 Fisher-Yates 置换，打乱字符顺序
//!
//! 非加密安全，仅用于避免明文密钥直接出现在代码与反编译结果中。
//! 解密为加密的逆过程：逆置换 → Hex 解码 → XOR 还原。

/// XOR 密钥（单字节）。
const XOR_KEY: u8 = 0xA5;

/// LCG 初始状态（固定 seed 保证加解密一致）。
const PERM_SEED: u32 = 0x5A82_7999;

/// 加密：明文 → 混淆后的十六进制字符串。
pub fn obfuscate(plain: &str) -> String {
    let bytes = plain.as_bytes();
    // 1. XOR
    let xored: Vec<u8> = bytes.iter().map(|b| b ^ XOR_KEY).collect();
    // 2. 十六进制编码
    let hex: Vec<char> = xored
        .iter()
        .flat_map(|b| {
            let s = format!("{:02x}", b);
            s.chars().collect::<Vec<_>>()
        })
        .collect();
    // 3. 置换：shuffled[perm[i]] = hex[i]
    let perm = build_permutation(hex.len());
    let mut shuffled = vec!['\0'; hex.len()];
    for (i, &c) in hex.iter().enumerate() {
        shuffled[perm[i]] = c;
    }
    shuffled.into_iter().collect()
}

/// 解密：混淆后的十六进制字符串 → 明文。
pub fn deobfuscate(cipher: &str) -> String {
    let chars: Vec<char> = cipher.chars().collect();
    let perm = build_permutation(chars.len());
    // 1. 逆置换：加密时 shuffled[perm[i]] = hex[i]，故 hex[i] = shuffled[perm[i]]
    let mut hex = vec!['\0'; chars.len()];
    for i in 0..chars.len() {
        hex[i] = chars[perm[i]];
    }
    // 2. 十六进制解码
    let hex_str: String = hex.into_iter().collect();
    let bytes: Vec<u8> = (0..hex_str.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex_str[i..i + 2], 16)
                .expect("混淆字符串包含非法十六进制字符")
        })
        .collect();
    // 3. XOR 还原
    let plain: Vec<u8> = bytes.iter().map(|b| b ^ XOR_KEY).collect();
    String::from_utf8(plain).expect("解密结果不是合法 UTF-8")
}

/// 用固定 seed 的 LCG 生成 Fisher-Yates 置换。
///
/// 返回 `0..len` 的一个排列。相同 `len` 总是产生相同排列，
/// 保证加解密一致。
fn build_permutation(len: usize) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..len).collect();
    if len < 2 {
        return perm;
    }
    let mut state = PERM_SEED;
    for i in (1..len).rev() {
        // LCG: state = state * 1103515245 + 12345 (glibc 参数)
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let j = (state as usize) % (i + 1);
        perm.swap(i, j);
    }
    perm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_plain_text() {
        // 使用与实际 key 相同长度的测试字符串，避免在测试中暴露明文 key
        let plain = "test-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let cipher = obfuscate(plain);
        assert_ne!(cipher, plain, "密文不应等于明文");
        assert!(!cipher.contains("test-"), "密文不应包含明文片段");
        let recovered = deobfuscate(&cipher);
        assert_eq!(recovered, plain);
    }

    #[test]
    fn roundtrip_empty_string() {
        let cipher = obfuscate("");
        assert_eq!(cipher, "");
        assert_eq!(deobfuscate(""), "");
    }

    #[test]
    fn roundtrip_short_string() {
        let plain = "a";
        let cipher = obfuscate(plain);
        assert_eq!(deobfuscate(&cipher), plain);
    }

    #[test]
    fn roundtrip_unicode() {
        let plain = "你好世界🌍";
        let cipher = obfuscate(plain);
        assert_eq!(deobfuscate(&cipher), plain);
    }

    #[test]
    fn cipher_is_printable_hex() {
        let plain = "secret-key-12345";
        let cipher = obfuscate(plain);
        // 密文应只包含十六进制字符
        assert!(cipher.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn different_lengths_produce_valid_permutations() {
        // 验证不同长度的字符串都能正确加解密
        for len in 0..=64 {
            let plain = "a".repeat(len);
            let cipher = obfuscate(&plain);
            assert_eq!(deobfuscate(&cipher), plain, "len={len} 加解密失败");
        }
    }

    #[test]
    fn permutation_is_consistent() {
        // 相同长度应产生相同置换
        let p1 = build_permutation(32);
        let p2 = build_permutation(32);
        assert_eq!(p1, p2);
    }

    #[test]
    fn permutation_is_valid_permutation() {
        let len = 20;
        let perm = build_permutation(len);
        // 应是 0..len 的排列
        let mut sorted = perm.clone();
        sorted.sort();
        assert_eq!(sorted, (0..len).collect::<Vec<_>>());
    }

    /// 验证 modelscope.rs 中的混淆密文能正确解密。
    #[test]
    fn modelscope_cipher_decrypts_correctly() {
        let cipher = "087cc9991899369903127d58999599c61c96694908183801cc904c0990991dcdd88893899c9398";
        let plain = deobfuscate(cipher);
        assert!(plain.starts_with("ms-"), "解密结果应以 'ms-' 开头");
        assert_eq!(plain.len(), 39, "ModelScope token 长度应为 39");
    }
}
