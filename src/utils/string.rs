/// 提供 String / &str 的扩展方法
pub trait StringUtilsTrait {
    /// 安全截取前 `max_chars` 个字符（Unicode 安全，不会截断在字符中间）
    ///
    /// 如果不足 `max_chars`，则返回完整字符串。
    fn safe_substring(&self, max_chars: usize) -> &str;
}

impl StringUtilsTrait for str {
    fn safe_substring(&self, max_chars: usize) -> &str {
        let mut end = self.len();
        for (i, (pos, _)) in self.char_indices().enumerate() {
            if i == max_chars {
                end = pos;
                break;
            }
        }
        &self[..end]
    }
}

impl StringUtilsTrait for String {
    fn safe_substring(&self, max_chars: usize) -> &str {
        self.as_str().safe_substring(max_chars)
    }
}
