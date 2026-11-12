//! Text back from where the glyphs sit on the page, for producers that write glyphs in drawing
//! order: lines by their height, each line's glyphs by their place from left to right, then the
//! line from visual order into reading order. A ligature's letters (one glyph, several letters,
//! in reading order already) stay together, diacritics stay on their letter, and word spaces are
//! the ones PDFium found, or a wide gap.

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub ch: char,
    pub left: f64,
    pub right: f64,
    pub bottom: f64,
    pub top: f64,
    /// PDFium had a space just before this character
    pub space_before: bool,
}

pub fn is_mark(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}' | '\u{0610}'..='\u{061A}' | '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{06D6}'..='\u{06DC}' | '\u{06DF}'..='\u{06E4}' | '\u{06E7}' | '\u{06E8}' | '\u{06EA}'..='\u{06ED}')
}

fn is_rtl(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}') && !matches!(c, '\u{0660}'..='\u{0669}' | '\u{06F0}'..='\u{06F9}')
}

/// Shown left to right inside right-to-left text: Latin letters and every kind of digit.
fn is_ltr(c: char) -> bool {
    c.is_ascii_alphanumeric() || ('\u{00C0}'..='\u{024F}').contains(&c) || matches!(c, '\u{0660}'..='\u{0669}' | '\u{06F0}'..='\u{06F9}')
}

/// Between two left-to-right pieces, these belong to them: "1,200", "1.8.0", "Windows 11".
fn joins_ltr(c: char) -> bool {
    matches!(c, ' ' | '.' | ',' | ':' | '%' | '-' | '/' | '+' | '\'' | '_' | '#' | '@' | '&')
}

/// One or more letters drawn as one glyph, with the diacritics on it.
#[derive(Clone, Debug)]
struct Cluster {
    chars: Vec<char>,
    marks: Vec<char>,
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
    first: usize,
    last: usize,
    space_before: bool,
}

impl Cluster {
    fn cx(&self) -> f64 {
        (self.left + self.right) / 2.0
    }
    fn cy(&self) -> f64 {
        (self.bottom + self.top) / 2.0
    }
    fn h(&self) -> f64 {
        (self.top - self.bottom).max(0.1)
    }
    fn lead(&self) -> char {
        self.chars[0]
    }
}

/// Which of the rules are used. `CHOSEN` is what the app uses; the trial (tools/trials/pdf)
/// compares the others.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    pub ligatures: bool,
    pub tight_lines: bool,
    pub pdfium_spaces: bool,
    pub gap_spaces: bool,
}

impl Rules {
    /// Best on the trial's PDFs: PDFium's own word spaces, lines by height, ligatures not kept whole.
    pub const CHOSEN: Rules = Rules { ligatures: false, tight_lines: false, pdfium_spaces: true, gap_spaces: false };
}

pub fn text(glyphs: &[Glyph], r: Rules) -> String {
    // letters, ligatures kept whole (the same box, one after the other)
    let mut clusters: Vec<Cluster> = Vec::new();
    let mut marks: Vec<(usize, Glyph)> = Vec::new();
    for (i, g) in glyphs.iter().enumerate() {
        if g.ch.is_whitespace() || g.ch.is_control() {
            continue;
        }
        if is_mark(g.ch) {
            marks.push((i, *g));
            continue;
        }
        if let Some(c) = clusters.last_mut() {
            if r.ligatures && c.last + 1 == i && (c.left - g.left).abs() < 0.01 && (c.right - g.right).abs() < 0.01 {
                c.chars.push(g.ch);
                c.last = i;
                continue;
            }
        }
        clusters.push(Cluster { chars: vec![g.ch], marks: vec![], left: g.left, right: g.right, bottom: g.bottom, top: g.top, first: i, last: i, space_before: g.space_before });
    }
    // rows: a cluster joins the row whose first cluster is at its own height
    let mut rows: Vec<Vec<Cluster>> = Vec::new();
    for c in clusters {
        let row = rows.iter_mut().rev().find(|l| {
            let m = &l[0];
            if r.tight_lines {
                (c.cy() - m.cy()).abs() < 0.35 * m.h()
            } else {
                c.cy() >= m.bottom - 0.15 * m.h() && c.cy() <= m.top + 0.15 * m.h()
            }
        });
        match row {
            Some(l) => l.push(c),
            None => rows.push(vec![c]),
        }
    }
    // lines: a row cut where a gap is wider than a word space can be (columns, a table's cells),
    // kept in the order the file has them, which is the reading order of columns
    let mut lines: Vec<Vec<Cluster>> = Vec::new();
    for mut row in rows {
        row.sort_by(|a, b| a.cx().total_cmp(&b.cx()));
        let mut heights: Vec<f64> = row.iter().map(Cluster::h).collect();
        heights.sort_by(f64::total_cmp);
        let size = heights[heights.len() / 2];
        let mut current: Vec<Cluster> = Vec::new();
        for c in row {
            if current.last().is_some_and(|p: &Cluster| c.left - p.right > 1.5 * size) {
                lines.push(std::mem::take(&mut current));
            }
            current.push(c);
        }
        lines.push(current);
    }
    lines.sort_by_key(|l| l.iter().map(|c| c.first).min().unwrap_or(0));
    // each diacritic on the nearest letter at its height
    for (_, m) in marks {
        let (mx, my) = ((m.left + m.right) / 2.0, (m.bottom + m.top) / 2.0);
        let mut best: Option<(usize, usize, f64)> = None;
        for (li, l) in lines.iter().enumerate() {
            for (ci, c) in l.iter().enumerate() {
                if (c.cy() - my).abs() > 0.8 * c.h() {
                    continue;
                }
                let d = (c.cx() - mx).abs() + (c.cy() - my).abs() * 0.5;
                if best.map_or(true, |b| d < b.2) {
                    best = Some((li, ci, d));
                }
            }
        }
        if let Some((li, ci, _)) = best {
            lines[li][ci].marks.push(m.ch);
        }
    }
    lines.into_iter().map(|l| line_text(l, r)).collect::<Vec<_>>().join("\n")
}

/// What a line is made of before it is put in reading order: clusters and spaces.
#[derive(Clone)]
enum Piece {
    C(Vec<char>, Vec<char>),
    Space,
}

impl Piece {
    fn lead(&self) -> char {
        match self {
            Piece::C(c, _) => c[0],
            Piece::Space => ' ',
        }
    }
}

fn line_text(mut l: Vec<Cluster>, r: Rules) -> String {
    l.sort_by(|a, b| a.cx().total_cmp(&b.cx()));
    let size = {
        let mut h: Vec<f64> = l.iter().map(Cluster::h).collect();
        h.sort_by(f64::total_cmp);
        h[h.len() / 2]
    };
    let mut visual: Vec<Piece> = Vec::new();
    for (k, c) in l.iter().enumerate() {
        if let Some(p) = k.checked_sub(1).map(|j| &l[j]) {
            // neighbours on the page that were neighbours in the text, with a space between there
            let (x, y) = if p.last < c.first { (p, c) } else { (c, p) };
            let found = y.space_before && (y.first == x.last + 1 || y.first == x.last + 2);
            let gap = c.left - p.right;
            let script_changes = is_rtl(c.lead()) != is_rtl(p.lead()) && (c.lead().is_alphanumeric() && p.lead().is_alphanumeric());
            if (r.pdfium_spaces && found) || (r.gap_spaces && gap > 0.1 * size) || (script_changes && gap > 0.08 * size) || gap > 0.3 * size {
                visual.push(Piece::Space);
            }
        }
        visual.push(Piece::C(c.chars.clone(), c.marks.clone()));
    }
    let rtl = l.iter().filter(|c| is_rtl(c.lead())).count();
    let ltr = l.iter().filter(|c| c.lead().is_alphabetic() && !is_rtl(c.lead())).count();
    let logical = if rtl >= ltr && rtl > 0 { rtl_line(visual) } else { ltr_line(visual) };
    let mut out = String::new();
    for p in logical {
        match p {
            Piece::C(chars, marks) => {
                out.extend(chars);
                out.extend(marks);
            }
            Piece::Space => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
        }
    }
    out.trim().to_string()
}

/// Reading order of a right-to-left line shown left to right: all reversed, then each
/// left-to-right piece turned back, and mirrored signs (brackets, quotes) turned round.
fn rtl_line(mut v: Vec<Piece>) -> Vec<Piece> {
    v.reverse();
    let mut i = 0;
    while i < v.len() {
        if is_ltr(v[i].lead()) {
            let mut j = i + 1;
            let mut end = i + 1;
            while j < v.len() && (is_ltr(v[j].lead()) || joins_ltr(v[j].lead())) {
                if is_ltr(v[j].lead()) || v[j].lead() == '%' {
                    end = j + 1;
                }
                j += 1;
            }
            v[i..end].reverse();
            i = end;
        } else {
            if let Piece::C(chars, _) = &mut v[i] {
                if chars.len() == 1 {
                    chars[0] = mirror(chars[0]);
                }
            }
            i += 1;
        }
    }
    v
}

/// A left-to-right line: right-to-left pieces inside it turned back.
fn ltr_line(mut v: Vec<Piece>) -> Vec<Piece> {
    let mut i = 0;
    while i < v.len() {
        if is_rtl(v[i].lead()) {
            let mut j = i + 1;
            let mut end = i + 1;
            while j < v.len() && (is_rtl(v[j].lead()) || v[j].lead() == ' ') {
                if is_rtl(v[j].lead()) {
                    end = j + 1;
                }
                j += 1;
            }
            v[i..end].reverse();
            i = end;
        } else {
            i += 1;
        }
    }
    v
}

fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Glyphs of one line laid out as a PDF draws them: left to right on the page, 10 units each.
    fn drawn(visual: &str, y: f64, spaces_at: &[usize]) -> Vec<Glyph> {
        visual
            .chars()
            .enumerate()
            .filter(|(_, c)| *c != ' ')
            .map(|(i, ch)| Glyph { ch, left: i as f64 * 10.0, right: i as f64 * 10.0 + 10.0, bottom: y, top: y + 12.0, space_before: spaces_at.contains(&i) })
            .collect()
    }

    #[test]
    fn a_right_to_left_line_drawn_left_to_right_comes_back_in_reading_order() {
        // "النسخة الجديدة Dawwin 1.8" on the page: the Arabic at the right, the rest at the left
        let g = drawn("Dawwin 1.8 ةديدجلا ةخسنلا", 0.0, &[]);
        assert_eq!(text(&g, Rules::CHOSEN), "النسخة الجديدة Dawwin 1.8");
    }

    #[test]
    fn glyphs_in_any_order_in_the_file_sit_where_they_are_drawn() {
        let mut g = drawn("ملاعلا ابحرم", 0.0, &[]);
        g.reverse();
        g.swap(0, 3);
        assert_eq!(text(&g, Rules::CHOSEN), "مرحبا العالم");
    }

    #[test]
    fn lines_by_their_height_and_diacritics_on_their_letter() {
        let mut g = drawn("بتك", 100.0, &[]);
        g.extend(drawn("English", 80.0, &[]));
        // a fatha above the middle letter, written last in the file
        g.push(Glyph { ch: '\u{064E}', left: 12.0, right: 18.0, bottom: 109.0, top: 115.0, space_before: false });
        assert_eq!(text(&g, Rules::CHOSEN), "كتَب\nEnglish");
    }

    #[test]
    fn brackets_turn_round_in_right_to_left_text() {
        // "(نص)" drawn right to left: the opening bracket at the right, drawn as ')'
        let g = drawn("(صن)", 0.0, &[]);
        assert_eq!(text(&g, Rules::CHOSEN), "(نص)");
    }
}
