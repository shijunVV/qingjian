//! 英文模式（Caps Lock 亮着）的候选：敲的字母不当拼音，按英文词表给精确词、前缀补全和拼错一个字母的纠正。
//!
//! 空格与标点仍原样上屏敲的字母，候选只在按 Tab 或方向键选中时才用上，所以候选再多也不碍事。

mod edit;

pub use edit::within_one_edit;
use qingjian_dictionary::WordList;

/// 拼错纠正至少要几个字母：三个字母以内一处编辑能变出太多词，只补全不纠正。
pub const MIN_CORRECTION_LETTERS: usize = 4;

/// 按敲的字母给英文候选：精确词（按词表里的大小写）第一，然后是前缀补全、再是差一处编辑的词，
/// 后两组各自按用户选过的次数、词频排序。大小写跟着敲的走：`Compa` 出 Company，`COMPA` 出 COMPANY。
/// 敲的全小写而精确词首字母大写（词表里的 OR、单字母 S、May，个人表学过的 A）时，敲的原样排第一、词表写法跟在后面：
/// 空格选的是第一个，大写该由 Shift 决定；iPhone 这类首字母小写的写法仍按词表。
///
/// `lists` 是若干词表，前面的优先（用户自己打过的词在前、随包词表在后）：精确词取第一个有的，补全与纠正合在一起排。
pub fn suggest(
    lists: &[&WordList],
    typed: &str,
    weight: impl Fn(&str) -> u32,
    limit: usize,
) -> Vec<String> {
    let code = typed.to_ascii_lowercase();
    if code.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut result: Vec<String> = Vec::with_capacity(limit);
    if let Some(word) = lists.iter().find_map(|words| words.get(&code)) {
        let word = adapt_case(word, typed);
        if typed == code && word.starts_with(|c: char| c.is_ascii_uppercase()) {
            result.push(code.clone());
        }
        result.push(word);
        result.truncate(limit);
    }
    let ranked = |mut hits: Vec<(&str, u32)>, result: &mut Vec<String>| {
        hits.sort_by(|a, b| {
            weight(b.0)
                .cmp(&weight(a.0))
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.0.cmp(b.0))
        });
        for (word, _) in hits {
            if result.len() >= limit {
                break;
            }
            let word = adapt_case(word, typed);
            if !result.contains(&word) {
                result.push(word);
            }
        }
    };
    let entries = || lists.iter().flat_map(|words| words.entries());
    let completions: Vec<(&str, u32)> = entries()
        .filter(|(entry_code, _, _)| entry_code.len() > code.len() && entry_code.starts_with(&code))
        .map(|(_, word, frequency)| (word, frequency))
        .collect();
    ranked(completions, &mut result);
    // 带数字或符号的（`foo1`、`x_y`）是标识符不是拼错的词，不去猜
    let letters_only = code.bytes().all(|b| b.is_ascii_lowercase());
    if letters_only && code.len() >= MIN_CORRECTION_LETTERS && result.len() < limit {
        let corrections: Vec<(&str, u32)> = entries()
            .filter(|(entry_code, _, _)| {
                entry_code.len().abs_diff(code.len()) <= 1
                    && !entry_code.starts_with(&code)
                    && within_one_edit(entry_code, &code)
            })
            .map(|(_, word, frequency)| (word, frequency))
            .collect();
        ranked(corrections, &mut result);
    }
    result
}

/// 词表里的写法按敲的大小写调整：全大写就全大写，首字母大写就首字母大写，其余照词表（iPhone 仍是 iPhone）。
fn adapt_case(word: &str, typed: &str) -> String {
    let mut chars = typed.chars();
    let first_upper = chars.next().is_some_and(|c| c.is_ascii_uppercase());
    let rest_upper = typed.len() >= 2 && typed.chars().all(|c| !c.is_ascii_lowercase());
    if first_upper && rest_upper {
        word.to_ascii_uppercase()
    } else if first_upper {
        let mut out = String::with_capacity(word.len());
        let mut word_chars = word.chars();
        if let Some(c) = word_chars.next() {
            out.extend(c.to_uppercase());
        }
        out.push_str(word_chars.as_str());
        out
    } else {
        word.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words() -> WordList {
        WordList::parse(
            "company\tcompany\t900\ncompare\tcompare\t500\ncompass\tcompass\t300\ncomma\tcomma\t100\n\
             iPhone\tiphone\t800\nhello\thello\t1000\nhollow\thollow\t200\nhelp\thelp\t700\n",
        )
        .unwrap()
    }

    #[test]
    fn exact_then_completions_then_corrections() {
        let list = words();
        assert_eq!(
            suggest(&[&list], "comp", |_| 0, 9),
            ["company", "compare", "compass"]
        );
        // 精确词第一，词表写法优先
        assert_eq!(suggest(&[&list], "iphone", |_| 0, 9), ["iPhone"]);
        // 拼错一个字母：helo → hello / help（差一处），hollow 差两处不算
        assert_eq!(suggest(&[&list], "helo", |_| 0, 9), ["hello", "help"]);
        // 三个字母以内不纠正；带数字的是标识符，也不纠正
        assert!(suggest(&[&list], "hlo", |_| 0, 9).is_empty());
        assert!(suggest(&[&list], "hel1", |_| 0, 9).is_empty());
        assert!(suggest(&[&list], "", |_| 0, 9).is_empty());
    }

    #[test]
    fn user_choices_and_limit_shape_the_order() {
        let list = words();
        let weight = |t: &str| u32::from(t == "compass");
        assert_eq!(suggest(&[&list], "comp", weight, 2), ["compass", "company"]);
    }

    #[test]
    fn case_follows_the_typed_letters() {
        let list = words();
        assert_eq!(suggest(&[&list], "Comp", |_| 0, 1), ["Company"]);
        assert_eq!(suggest(&[&list], "COMP", |_| 0, 1), ["COMPANY"]);
        assert_eq!(suggest(&[&list], "Iphone", |_| 0, 1), ["IPhone"]);
        assert_eq!(adapt_case("hello", "H"), "Hello");
    }

    #[test]
    fn lowercase_typing_puts_the_typed_letters_before_capitalized_forms() {
        // 随包词表把 or 写成 OR、单字母写成大写，个人表里学过 A：敲小写时敲的原样第一，词表写法紧跟其后
        let list = WordList::parse(
            "OR\tor\t6540\norder\torder\t5000\nS\ts\t5860\nMay\tmay\t5980\nmaybe\tmaybe\t5000\n",
        )
        .unwrap();
        assert_eq!(suggest(&[&list], "or", |_| 0, 9), ["or", "OR", "order"]);
        assert_eq!(suggest(&[&list], "s", |_| 0, 9), ["s", "S"]);
        assert_eq!(suggest(&[&list], "may", |_| 0, 9), ["may", "May", "maybe"]);
        assert_eq!(suggest(&[&list], "or", |_| 0, 1), ["or"]);
        let user = WordList::parse("A\ta\t76\n").unwrap();
        let main = WordList::parse("a\ta\t7360\nand\tand\t7000\n").unwrap();
        assert_eq!(suggest(&[&user, &main], "a", |_| 0, 9), ["a", "A", "and"]);
        // 按着 Shift 敲的照旧跟着敲的大小写；首字母小写的写法（iPhone）仍按词表
        assert_eq!(suggest(&[&list], "OR", |_| 0, 1), ["OR"]);
        assert_eq!(suggest(&[&list], "May", |_| 0, 1), ["May"]);
        assert_eq!(suggest(&[&words()], "iphone", |_| 0, 9), ["iPhone"]);
    }

    #[test]
    fn earlier_lists_win_for_exact_words_and_pool_for_completions() {
        let user = WordList::parse("gist\tgist\t3\n").unwrap();
        let main = WordList::parse("gist\tgist\t0\ngiant\tgiant\t900\ngift\tgift\t500\n").unwrap();
        // 用户表里的 gist 精确命中，补全从两张表合起来按词频排
        assert_eq!(
            suggest(&[&user, &main], "gi", |_| 0, 9),
            ["giant", "gift", "gist"]
        );
        // 精确词后面跟着差一处编辑的 gift
        assert_eq!(suggest(&[&user, &main], "gist", |_| 0, 9), ["gist", "gift"]);
        assert_eq!(suggest(&[&main], "gis", |_| 0, 9), ["gist"]);
    }
}
