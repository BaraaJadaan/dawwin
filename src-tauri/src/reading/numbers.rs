//! Numbers, dates, times, percentages and money in words, as a person reads them out, in the
//! language of the text around them. Arabic numbers take the forms heard in spoken Standard Arabic
//! ("عشرين", "اثنين", "مئتين"): the case a sentence needs cannot be told without parsing it, and
//! these are right after a preposition and in most of what people say.

use super::text::Lang;

const EN_ONES: [&str; 20] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen", "fourteen",
    "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
];
const EN_TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
const EN_MONTHS: [&str; 12] = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];
const EN_MONTH_NAMES: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

fn en_under_100(n: u64) -> String {
    if n < 20 {
        EN_ONES[n as usize].to_string()
    } else if n % 10 == 0 {
        EN_TENS[(n / 10) as usize].to_string()
    } else {
        format!("{}-{}", EN_TENS[(n / 10) as usize], EN_ONES[(n % 10) as usize])
    }
}

fn en_under_1000(n: u64) -> String {
    match (n / 100, n % 100) {
        (0, r) => en_under_100(r),
        (h, 0) => format!("{} hundred", EN_ONES[h as usize]),
        (h, r) => format!("{} hundred {}", EN_ONES[h as usize], en_under_100(r)),
    }
}

pub fn en_cardinal(n: u64) -> String {
    if n < 1000 {
        return en_under_1000(n);
    }
    let scales = [(1_000_000_000_000, "trillion"), (1_000_000_000, "billion"), (1_000_000, "million"), (1000, "thousand")];
    let mut parts = Vec::new();
    let mut rest = n;
    for (size, name) in scales {
        if rest >= size {
            parts.push(format!("{} {name}", en_cardinal(rest / size)));
            rest %= size;
        }
    }
    if rest > 0 {
        parts.push(en_under_1000(rest));
    }
    parts.join(" ")
}

pub fn en_ordinal(n: u64) -> String {
    let c = en_cardinal(n);
    let (head, last) = match c.rfind([' ', '-']) {
        Some(i) => (&c[..=i], &c[i + 1..]),
        None => ("", c.as_str()),
    };
    let last = match last {
        "one" => "first".to_string(),
        "two" => "second".to_string(),
        "three" => "third".to_string(),
        "five" => "fifth".to_string(),
        "eight" => "eighth".to_string(),
        "nine" => "ninth".to_string(),
        "twelve" => "twelfth".to_string(),
        w if w.ends_with('y') => format!("{}ieth", &w[..w.len() - 1]),
        w => format!("{w}th"),
    };
    format!("{head}{last}")
}

/// "nineteen eighty-four", "two thousand five", "twenty twenty-six".
pub fn en_year(n: u64) -> String {
    match n {
        2000..=2009 => en_cardinal(n),
        1100..=2099 if n % 100 == 0 => format!("{} hundred", en_under_100(n / 100)),
        1100..=2099 => {
            let r = n % 100;
            format!("{} {}", en_under_100(n / 100), if r < 10 { format!("oh {}", EN_ONES[r as usize]) } else { en_under_100(r) })
        }
        _ => en_cardinal(n),
    }
}

const AR_ONES: [&str; 11] = ["صفر", "واحد", "اثنين", "ثلاثة", "أربعة", "خمسة", "ستة", "سبعة", "ثمانية", "تسعة", "عشرة"];
const AR_TENS: [&str; 10] = ["", "عشرة", "عشرين", "ثلاثين", "أربعين", "خمسين", "ستين", "سبعين", "ثمانين", "تسعين"];
const AR_HUNDREDS: [&str; 10] = ["", "مئة", "مئتين", "ثلاثمئة", "أربعمئة", "خمسمئة", "ستمئة", "سبعمئة", "ثمانمئة", "تسعمئة"];

fn ar_under_100(n: u64) -> String {
    match n {
        0..=10 => AR_ONES[n as usize].to_string(),
        11 => "أحد عشر".to_string(),
        12 => "اثني عشر".to_string(),
        13..=19 => format!("{} عشر", AR_ONES[(n % 10) as usize]),
        _ if n % 10 == 0 => AR_TENS[(n / 10) as usize].to_string(),
        _ => format!("{} و{}", AR_ONES[(n % 10) as usize], AR_TENS[(n / 10) as usize]),
    }
}

fn ar_under_1000(n: u64) -> String {
    match (n / 100, n % 100) {
        (0, r) => ar_under_100(r),
        (h, 0) => AR_HUNDREDS[h as usize].to_string(),
        (h, r) => format!("{} و{}", AR_HUNDREDS[h as usize], ar_under_100(r)),
    }
}

/// A count of thousands (millions…): "ألف", "ألفين", "ثلاثة آلاف", "خمسة عشر ألفاً", "مئتي ألف".
fn ar_scale(count: u64, one: &str, two: &str, few: &str, many: &str) -> String {
    match count {
        1 => one.to_string(),
        2 => two.to_string(),
        3..=10 => format!("{} {few}", ar_cardinal(count)),
        _ => {
            let words = ar_cardinal(count);
            // "مئتين" before what it counts is "مئتي"
            let words = words.strip_suffix("مئتين").map_or(words.clone(), |w| format!("{w}مئتي"));
            match count % 100 {
                3..=10 => format!("{words} {few}"),
                11..=99 => format!("{words} {many}"),
                _ => format!("{words} {one}"),
            }
        }
    }
}

pub fn ar_cardinal(n: u64) -> String {
    if n < 1000 {
        return ar_under_1000(n);
    }
    let scales: [(u64, [&str; 4]); 3] = [
        (1_000_000_000, ["مليار", "مليارين", "مليارات", "ملياراً"]),
        (1_000_000, ["مليون", "مليونين", "ملايين", "مليوناً"]),
        (1000, ["ألف", "ألفين", "آلاف", "ألفاً"]),
    ];
    let mut parts = Vec::new();
    let mut rest = n;
    for (size, [one, two, few, many]) in scales {
        if rest >= size {
            parts.push(ar_scale(rest / size, one, two, few, many));
            rest %= size;
        }
    }
    if rest > 0 {
        parts.push(ar_under_1000(rest));
    }
    parts.join(" و")
}

const AR_ORDINAL: [&str; 11] = ["", "الأول", "الثاني", "الثالث", "الرابع", "الخامس", "السادس", "السابع", "الثامن", "التاسع", "العاشر"];

/// The day of a month: "الأول", "الثاني عشر", "الحادي والعشرين".
pub fn ar_day(n: u64) -> String {
    let unit = |u: u64| if u == 1 { "الحادي".to_string() } else { AR_ORDINAL[u as usize].to_string() };
    match n {
        1..=10 => AR_ORDINAL[n as usize].to_string(),
        11..=19 => format!("{} عشر", unit(n % 10)),
        20 => "العشرين".into(),
        30 => "الثلاثين".into(),
        21..=29 => format!("{} والعشرين", unit(n % 10)),
        31 => "الحادي والثلاثين".into(),
        _ => ar_cardinal(n),
    }
}

const AR_HOURS: [&str; 13] = ["", "الواحدة", "الثانية", "الثالثة", "الرابعة", "الخامسة", "السادسة", "السابعة", "الثامنة", "التاسعة", "العاشرة", "الحادية عشرة", "الثانية عشرة"];

fn ar_time(h: u64, m: u64) -> String {
    let hour = |h: u64| AR_HOURS[(if h % 12 == 0 { 12 } else { h % 12 }) as usize];
    match m {
        0 => hour(h).to_string(),
        15 => format!("{} والربع", hour(h)),
        20 => format!("{} والثلث", hour(h)),
        30 => format!("{} والنصف", hour(h)),
        40 => format!("{} إلا ثلثاً", hour(h + 1)),
        45 => format!("{} إلا ربعاً", hour(h + 1)),
        1 => format!("{} ودقيقة", hour(h)),
        2 => format!("{} ودقيقتين", hour(h)),
        3..=10 => format!("{} و{} دقائق", hour(h), ar_cardinal(m)),
        _ => format!("{} و{} دقيقة", hour(h), ar_cardinal(m)),
    }
}

fn en_time(h: u64, m: u64, before_ampm: bool) -> String {
    match m {
        0 if before_ampm => en_cardinal(h),
        0 => format!("{} o'clock", en_cardinal(h)),
        1..=9 => format!("{} oh {}", en_cardinal(h), EN_ONES[m as usize]),
        _ => format!("{} {}", en_cardinal(h), en_under_100(m)),
    }
}

const AR_MONTHS: &[&str] = &[
    "يناير", "فبراير", "مارس", "أبريل", "إبريل", "مايو", "يونيو", "يونيه", "يوليو", "يوليه", "أغسطس", "سبتمبر", "أكتوبر", "نوفمبر", "ديسمبر",
    "كانون", "شباط", "آذار", "نيسان", "أيار", "حزيران", "تموز", "آب", "أيلول", "تشرين",
    "محرم", "صفر", "ربيع", "جمادى", "رجب", "شعبان", "رمضان", "شوال", "ذو", "ذي",
];
const AR_MONTH_NAMES: [&str; 12] = ["يناير", "فبراير", "مارس", "أبريل", "مايو", "يونيو", "يوليو", "أغسطس", "سبتمبر", "أكتوبر", "نوفمبر", "ديسمبر"];

fn is_month(word: &str, lang: Lang) -> bool {
    let w = word.trim_end_matches(['.', ',', '،']);
    match lang {
        Lang::En => {
            let w = w.to_lowercase();
            w.len() >= 3 && EN_MONTHS.iter().any(|m| *m == w || (w.len() == 3 || w == "sept") && m.starts_with(&w))
        }
        Lang::Ar => AR_MONTHS.contains(&w.trim_start_matches('و')) || AR_MONTHS.contains(&w),
    }
}

struct Currency {
    marks: &'static [&'static str],
    en: (&'static str, &'static str),
    ar: [&'static str; 4],
}

const CURRENCIES: &[Currency] = &[
    Currency { marks: &["$", "USD", "US$"], en: ("dollar", "dollars"), ar: ["دولار", "دولارين", "دولارات", "دولاراً"] },
    Currency { marks: &["€", "EUR"], en: ("euro", "euros"), ar: ["يورو", "يورو", "يورو", "يورو"] },
    Currency { marks: &["£", "GBP"], en: ("pound", "pounds"), ar: ["جنيه", "جنيهين", "جنيهات", "جنيهاً"] },
    Currency { marks: &["SAR", "ر.س"], en: ("riyal", "riyals"), ar: ["ريال", "ريالين", "ريالات", "ريالاً"] },
    Currency { marks: &["AED", "د.إ"], en: ("dirham", "dirhams"), ar: ["درهم", "درهمين", "دراهم", "درهماً"] },
    Currency { marks: &["EGP", "ج.م"], en: ("Egyptian pound", "Egyptian pounds"), ar: ["جنيه", "جنيهين", "جنيهات", "جنيهاً"] },
    Currency { marks: &["KWD", "د.ك"], en: ("dinar", "dinars"), ar: ["دينار", "دينارين", "دنانير", "ديناراً"] },
];

fn money(n: u64, cents: u64, c: &Currency, lang: Lang) -> String {
    match lang {
        Lang::En => {
            let main = format!("{} {}", en_cardinal(n), if n == 1 { c.en.0 } else { c.en.1 });
            match (n, cents) {
                (_, 0) => main,
                (0, _) => format!("{} cent{}", en_cardinal(cents), if cents == 1 { "" } else { "s" }),
                _ => format!("{main} and {} cent{}", en_cardinal(cents), if cents == 1 { "" } else { "s" }),
            }
        }
        Lang::Ar => {
            let [one, two, few, many] = c.ar;
            let main = match n {
                1 => format!("{one} واحد"),
                2 => two.to_string(),
                _ => match n % 100 {
                    3..=10 => format!("{} {few}", ar_cardinal(n)),
                    11..=99 => format!("{} {many}", ar_cardinal(n)),
                    _ => format!("{} {one}", ar_cardinal(n)),
                },
            };
            if cents == 0 {
                main
            } else {
                format!("{main} و{} سنتاً", ar_cardinal(cents))
            }
        }
    }
}

fn cardinal(n: u64, lang: Lang) -> String {
    match lang {
        Lang::En => en_cardinal(n),
        Lang::Ar => ar_cardinal(n),
    }
}

fn digits_one_by_one(d: &str, lang: Lang) -> String {
    d.chars().filter_map(|c| c.to_digit(10)).map(|x| cardinal(x as u64, lang)).collect::<Vec<_>>().join(" ")
}

/// A number with its fraction: "three point one four", "ثلاثة فاصلة أربعة عشر".
fn decimal(int: u64, frac: &str, lang: Lang) -> String {
    let point = if lang == Lang::En { "point" } else { "فاصلة" };
    let frac_words = if lang == Lang::Ar && frac.len() <= 2 && !frac.starts_with('0') {
        cardinal(frac.parse().unwrap_or(0), lang)
    } else {
        digits_one_by_one(frac, lang)
    };
    format!("{} {point} {frac_words}", cardinal(int, lang))
}

/// The number that starts at `i`: (its digits without separators, its fraction, where it ends).
fn read_number(chars: &[char], i: usize) -> Option<(String, Option<String>, usize)> {
    let mut j = i;
    let mut int = String::new();
    while j < chars.len() {
        let c = chars[j];
        if c.is_ascii_digit() {
            int.push(c);
            j += 1;
        } else if c == ',' && !int.is_empty() && chars.get(j + 1..j + 4).is_some_and(|t| t.iter().all(|d| d.is_ascii_digit())) && !chars.get(j + 4).is_some_and(|d| d.is_ascii_digit()) {
            j += 1; // 1,250
        } else {
            break;
        }
    }
    if int.is_empty() {
        return None;
    }
    let mut frac = None;
    if chars.get(j) == Some(&'.') && chars.get(j + 1).is_some_and(|d| d.is_ascii_digit()) {
        let mut k = j + 1;
        let mut f = String::new();
        while k < chars.len() && chars[k].is_ascii_digit() {
            f.push(chars[k]);
            k += 1;
        }
        frac = Some(f);
        j = k;
    }
    Some((int, frac, j))
}

fn word_after(chars: &[char], mut j: usize) -> (String, usize) {
    while j < chars.len() && chars[j] == ' ' {
        j += 1;
    }
    let start = j;
    while j < chars.len() && !chars[j].is_whitespace() && !matches!(chars[j], ',' | '،' | ';' | '؛' | '!' | '?' | '؟' | ')') {
        j += 1;
    }
    (chars[start..j].iter().collect(), j)
}

fn last_word(s: &str) -> &str {
    s.trim_end().rsplit(|c: char| c.is_whitespace()).next().unwrap_or("")
}

fn currency_mark(s: &str) -> Option<&'static Currency> {
    let s = s.trim_end_matches(['.', ',', '،']);
    CURRENCIES.iter().find(|c| c.marks.contains(&s))
}

/// The text with its numbers in words, for a run in `lang`.
pub fn speakable(text: &str, lang: Lang) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev = if i > 0 { chars[i - 1] } else { ' ' };
        // a symbol before the amount: $66.50, €20, and Arabic's %15
        let symbol = ['$', '€', '£'].contains(&c).then(|| currency_mark(&c.to_string())).flatten();
        if (symbol.is_some() || c == '%') && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
            if let Some((int, frac, end)) = read_number(&chars, i + 1) {
                let n: u64 = int.parse().unwrap_or(0);
                if let Some(cur) = symbol {
                    let (scale, after) = word_after(&chars, end);
                    let scale_word = ["thousand", "million", "billion", "ألف", "مليون", "مليار"].contains(&scale.as_str());
                    if scale_word {
                        let amount = frac.as_deref().map_or(cardinal(n, lang), |f| decimal(n, f, lang));
                        let unit = if lang == Lang::En { cur.en.1 } else { cur.ar[0] };
                        out.push_str(&format!("{amount} {scale} {unit}"));
                        i = after;
                    } else {
                        let cents = frac.map_or(0, |f| format!("{f:0<2}")[..2].parse().unwrap_or(0));
                        out.push_str(&money(n, cents, cur, lang));
                        i = end;
                    }
                } else {
                    let amount = frac.as_deref().map_or(cardinal(n, lang), |f| decimal(n, f, lang));
                    out.push_str(&format!("{amount} {}", if lang == Lang::En { "percent" } else { "بالمئة" }));
                    i = end;
                }
                continue;
            }
        }
        // a minus sign
        if c == '-' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) && (prev.is_whitespace() || i == 0) {
            out.push_str(if lang == Lang::En { "minus " } else { "سالب " });
            i += 1;
            continue;
        }
        if !c.is_ascii_digit() || prev.is_ascii_digit() || prev.is_alphabetic() {
            out.push(c);
            i += 1;
            continue;
        }
        // 10:30
        if let Some((h, m, end)) = time_at(&chars, i) {
            let (next, _) = word_after(&chars, end);
            let ampm = ["a.m.", "p.m.", "am", "pm", "a.m", "p.m", "AM", "PM"].contains(&next.as_str());
            out.push_str(&match lang {
                Lang::En => en_time(h, m, ampm),
                Lang::Ar => ar_time(h, m),
            });
            i = end;
            continue;
        }
        // 2026-03-12 and 12/3/2026
        if let Some((d, mo, y, end)) = date_at(&chars, i, lang) {
            out.push_str(&match lang {
                Lang::En => format!("{} {}, {}", EN_MONTH_NAMES[mo - 1], en_ordinal(d), en_year(y)),
                Lang::Ar => format!("{} من {} عام {}", ar_day(d), AR_MONTH_NAMES[mo - 1], ar_cardinal(y)),
            });
            i = end;
            continue;
        }
        let Some((int, frac, end)) = read_number(&chars, i) else {
            out.push(c);
            i += 1;
            continue;
        };
        // a telephone number, a code: digit by digit
        if frac.is_none() && (int.len() > 15 || (int.len() >= 4 && int.starts_with('0'))) {
            out.push_str(&digits_one_by_one(&int, lang));
            i = end;
            continue;
        }
        let n: u64 = int.parse().unwrap_or(0);
        let had_commas = chars[i..end].contains(&',');
        // what follows: %, an ordinal ending, a currency, a month
        if chars.get(end) == Some(&'%') {
            let amount = frac.as_deref().map_or(cardinal(n, lang), |f| decimal(n, f, lang));
            out.push_str(&format!("{amount} {}", if lang == Lang::En { "percent" } else { "بالمئة" }));
            i = end + 1;
            continue;
        }
        let suffix: String = chars[end..chars.len().min(end + 2)].iter().collect::<String>().to_lowercase();
        if lang == Lang::En && frac.is_none() && ["st", "nd", "rd", "th"].contains(&suffix.as_str()) && !chars.get(end + 2).is_some_and(|c| c.is_alphanumeric()) {
            out.push_str(&en_ordinal(n));
            i = end + 2;
            continue;
        }
        let (next, next_end) = word_after(&chars, end);
        if let Some(cur) = currency_mark(&next).filter(|_| next.chars().all(|c| !c.is_lowercase())) {
            let cents = frac.map_or(0, |f| format!("{f:0<2}")[..2].parse().unwrap_or(0));
            out.push_str(&money(n, cents, cur, lang));
            i = next_end;
            continue;
        }
        let before = last_word(&out).to_string();
        if frac.is_none() && (1..=31).contains(&n) && int.len() <= 2 {
            if is_month(&before, lang) && lang == Lang::En {
                out.push_str(&en_ordinal(n)); // March 12
                i = end;
                continue;
            }
            if is_month(&next, lang) {
                out.push_str(&match lang {
                    Lang::En => format!("the {} of", en_ordinal(n)), // 12 March
                    Lang::Ar => format!("{} من", ar_day(n)), // 12 مارس
                });
                i = end;
                continue;
            }
        }
        if frac.is_none() && !had_commas && int.len() == 4 && (1100..=2099).contains(&n) {
            let after_month = is_month(before.trim_end_matches(','), lang);
            out.push_str(&match lang {
                Lang::En => en_year(n),
                Lang::Ar if after_month => format!("عام {}", ar_cardinal(n)),
                Lang::Ar => ar_cardinal(n),
            });
            i = end;
            continue;
        }
        // 10-20: a range
        if frac.is_none() && matches!(chars.get(end), Some('-') | Some('–')) && chars.get(end + 1).is_some_and(|d| d.is_ascii_digit()) {
            out.push_str(&cardinal(n, lang));
            out.push_str(if lang == Lang::En { " to " } else { " إلى " });
            i = end + 1;
            continue;
        }
        out.push_str(&frac.as_deref().map_or(cardinal(n, lang), |f| decimal(n, f, lang)));
        i = end;
    }
    out
}

fn time_at(chars: &[char], i: usize) -> Option<(u64, u64, usize)> {
    let d = |k: usize| chars.get(k).filter(|c| c.is_ascii_digit()).and_then(|c| c.to_digit(10)).map(u64::from);
    let (h, colon) = match (d(i), d(i + 1)) {
        (Some(a), Some(b)) => (a * 10 + b, i + 2),
        (Some(a), None) => (a, i + 1),
        _ => return None,
    };
    if chars.get(colon) != Some(&':') {
        return None;
    }
    let (m1, m2) = (d(colon + 1)?, d(colon + 2)?);
    if d(colon + 3).is_some() || h > 23 {
        return None;
    }
    let m = m1 * 10 + m2;
    (m < 60).then_some((h, m, colon + 3))
}

fn date_at(chars: &[char], i: usize, lang: Lang) -> Option<(u64, usize, u64, usize)> {
    let mut parts = Vec::new();
    let mut seps = Vec::new();
    let mut j = i;
    loop {
        let start = j;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j == start {
            return None;
        }
        parts.push(chars[start..j].iter().collect::<String>());
        if parts.len() == 3 {
            break;
        }
        match chars.get(j) {
            Some(&s) if (s == '-' || s == '/' || s == '.') && chars.get(j + 1).is_some_and(|c| c.is_ascii_digit()) => {
                seps.push(s);
                j += 1;
            }
            _ => return None,
        }
    }
    if chars.get(j).is_some_and(|c| c.is_ascii_digit()) || seps[0] != seps[1] {
        return None;
    }
    let n: Vec<u64> = parts.iter().map(|p| p.parse().unwrap_or(0)).collect();
    let (d, m, y) = if parts[0].len() == 4 {
        (n[2], n[1], n[0]) // 2026-03-12
    } else if parts[2].len() == 4 || parts[2].len() == 2 {
        let y = if parts[2].len() == 2 { 2000 + n[2] } else { n[2] };
        // day first, unless that cannot be, or the text is English and the month could come first
        if n[1] > 12 || (lang == Lang::En && n[0] <= 12 && seps[0] == '/') {
            (n[1], n[0], y)
        } else {
            (n[0], n[1], y)
        }
    } else {
        return None;
    };
    ((1..=31).contains(&d) && (1..=12).contains(&m) && (1000..=2999).contains(&y)).then_some((d, m as usize, y, j))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_numbers() {
        assert_eq!(en_cardinal(0), "zero");
        assert_eq!(en_cardinal(24), "twenty-four");
        assert_eq!(en_cardinal(105), "one hundred five");
        assert_eq!(en_cardinal(4500), "four thousand five hundred");
        assert_eq!(en_cardinal(1_250_000), "one million two hundred fifty thousand");
        assert_eq!(en_ordinal(12), "twelfth");
        assert_eq!(en_ordinal(22), "twenty-second");
        assert_eq!(en_ordinal(40), "fortieth");
        assert_eq!(en_year(2026), "twenty twenty-six");
        assert_eq!(en_year(2005), "two thousand five");
        assert_eq!(en_year(1984), "nineteen eighty-four");
        assert_eq!(en_year(1905), "nineteen oh five");
        assert_eq!(en_year(1900), "nineteen hundred");
    }

    #[test]
    fn arabic_numbers() {
        assert_eq!(ar_cardinal(7), "سبعة");
        assert_eq!(ar_cardinal(12), "اثني عشر");
        assert_eq!(ar_cardinal(15), "خمسة عشر");
        assert_eq!(ar_cardinal(24), "أربعة وعشرين");
        assert_eq!(ar_cardinal(250), "مئتين وخمسين");
        assert_eq!(ar_cardinal(2026), "ألفين وستة وعشرين");
        assert_eq!(ar_cardinal(1990), "ألف وتسعمئة وتسعين");
        assert_eq!(ar_cardinal(4500), "أربعة آلاف وخمسمئة");
        assert_eq!(ar_cardinal(15_000), "خمسة عشر ألفاً");
        assert_eq!(ar_cardinal(200_000), "مئتي ألف");
        assert_eq!(ar_cardinal(1_250_000), "مليون ومئتين وخمسين ألفاً");
        assert_eq!(ar_day(1), "الأول");
        assert_eq!(ar_day(12), "الثاني عشر");
        assert_eq!(ar_day(21), "الحادي والعشرين");
    }

    /// The bench's passages as written, and as tools/bench-voice/texts.json writes them out.
    #[test]
    fn the_bench_passages_come_out_as_written_out_there() {
        let texts: serde_json::Value = serde_json::from_str(include_str!("../../../tools/bench-voice/texts.json")).unwrap();
        let passage = |id: &str| -> Vec<String> {
            let p = texts["passages"].as_array().unwrap().iter().find(|p| p["id"] == id).unwrap();
            p["paragraphs"][0].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
        };
        for (lang, written, words) in [(Lang::Ar, "ar-numbers", "ar-numbers-words"), (Lang::En, "en-numbers", "en-numbers-words")] {
            for (a, b) in passage(written).iter().zip(passage(words)) {
                assert_eq!(speakable(&crate::reading::text::normalize(a), lang), b);
            }
        }
    }

    #[test]
    fn times_dates_money_and_percentages() {
        assert_eq!(speakable("at 10:05 or 10:00 or 9:00 p.m.", Lang::En), "at ten oh five or ten o'clock or nine p.m.");
        assert_eq!(speakable("الساعة 7:45 أو 11:15 أو 1:20", Lang::Ar), "الساعة الثامنة إلا ربعاً أو الحادية عشرة والربع أو الواحدة والثلث");
        assert_eq!(speakable("on 2026-03-12", Lang::En), "on March twelfth, twenty twenty-six");
        assert_eq!(speakable("يوم 25/12/2025", Lang::Ar), "يوم الخامس والعشرين من ديسمبر عام ألفين وخمسة وعشرين");
        assert_eq!(speakable("on 12 March", Lang::En), "on the twelfth of March");
        assert_eq!(speakable("the 1st and the 3rd", Lang::En), "the first and the third");
        assert_eq!(speakable("$1, $5 and €1.5 million", Lang::En), "one dollar, five dollars and one point five million euros");
        assert_eq!(speakable("بسعر 3 دولار أو $15 أو 100 SAR", Lang::Ar), "بسعر ثلاثة دولار أو خمسة عشر دولاراً أو مئة ريال");
        assert_eq!(speakable("نسبة %15 ثم 3.5%", Lang::Ar), "نسبة خمسة عشر بالمئة ثم ثلاثة فاصلة خمسة بالمئة");
        assert_eq!(speakable("pages 10-20, -5 degrees", Lang::En), "pages ten to twenty, minus five degrees");
        assert_eq!(speakable("call 0501234567", Lang::En), "call zero five zero one two three four five six seven");
        assert_eq!(speakable("1,250 people and 3.14", Lang::En), "one thousand two hundred fifty people and three point one four");
        // digits inside a word are the word's own
        assert_eq!(speakable("an mp3 file", Lang::En), "an mp3 file");
    }
}
