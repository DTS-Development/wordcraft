//! Word-processor font resolution: map a requested family + bold/italic to an available face,
//! with metric-compatible substitutes for common proprietary document fonts, and Word-style line
//! metrics (Windows ascent/descent, which is what Word uses for single line spacing).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use skrifa::raw::TableProvider;

use crate::{FaceRef, FontDb};

/// A requested font resolved to a face.
#[derive(Clone, Copy, Debug)]
pub struct Resolved {
    pub face: FaceRef,
    /// The face isn't bold but bold was asked for: embolden when drawing.
    pub synth_bold: bool,
    /// The face isn't italic but italic was asked for: slant when drawing.
    pub synth_italic: bool,
    /// The requested family wasn't available.
    pub substituted: bool,
}

/// The PPM build's name mapping (`ppm/font_mapping`): Office and Korean document fonts map onto the
/// bundled Office-compatible set, matched case-insensitively against an exact list. Other names
/// are Carlito, except the symbol fonts and the non-Latin scripts' names (Thai, Hebrew, Persian…),
/// which keep their chains; Korean names map to the bundled Korean set (NanumGothic, NanumMyeongjo, NanumGothicCoding, Black Han Sans), each chain ending in NanumGothic.
#[cfg(feature = "ppm-host")]
const PPM_CARLITO: &[&str] = &["Carlito", "Liberation Sans", "Source Sans 3"];

#[cfg(feature = "ppm-host")]
fn ppm_office_chain(f: &str) -> Option<&'static [&'static str]> {
    const CARLITO: &[&str] = PPM_CARLITO;
    const CALADEA: &[&str] = &["Caladea", "Carlito", "Source Sans 3"];
    const SANS: &[&str] = &["Liberation Sans", "Carlito", "Source Sans 3"];
    const SERIF: &[&str] = &["Liberation Serif", "Carlito", "Source Sans 3"];
    const MONO: &[&str] = &["Liberation Mono", "Carlito", "Source Sans 3"];
    const KOREAN: &[&str] = &["NanumGothic", "Carlito", "Liberation Sans", "Source Sans 3"];
    const MYEONGJO: &[&str] = &["NanumMyeongjo", "NanumGothic", "Carlito", "Liberation Sans", "Source Sans 3"];
    const CODING: &[&str] = &["NanumGothicCoding", "NanumGothic", "Carlito", "Liberation Sans", "Source Sans 3"];
    const HEADLINE: &[&str] = &["Black Han Sans", "NanumGothic", "Carlito", "Liberation Sans", "Source Sans 3"];
    const INTER: &[&str] = &["Inter", "Carlito", "Source Sans 3"];
    Some(match f {
        "calibri" | "calibri light" | "aptos" | "aptos display" | "aptos narrow" | "aptos light" | "aptos semibold" | "segoe ui"
        | "segoe ui light" | "segoe ui semibold" | "segoe ui semilight" => CARLITO,
        "cambria" | "constantia" | "palatino" | "palatino linotype" | "book antiqua" | "georgia" | "garamond" => CALADEA,
        "arial" | "arial narrow" | "helvetica" | "helvetica neue" => SANS,
        "times" | "times new roman" => SERIF,
        "courier" | "courier new" | "consolas" | "lucida console" => MONO,
        "inter" => INTER,
        // 고딕 → NanumGothic. noto·apple 이름은 예전부터 여기였다.
        "맑은 고딕"
        | "malgun gothic"
        | "굴림"
        | "gulim"
        | "새굴림"
        | "new gulim"
        | "돋움"
        | "dotum"
        | "함초롬돋움"
        | "hcr dotum"
        | "나눔고딕"
        | "nanumgothic"
        | "hy중고딕"
        | "hygothic-medium"
        | "noto sans kr"
        | "noto sans cjk kr"
        | "noto serif kr"
        | "apple sd gothic neo" => KOREAN,
        // 명조 → NanumMyeongjo, 없으면 NanumGothic(한글 보장).
        "바탕"
        | "batang"
        | "궁서"
        | "gungsuh"
        | "함초롬바탕"
        | "hcr batang"
        | "나눔명조"
        | "nanummyeongjo"
        | "hy신명조"
        | "hysinmyeongjo-medium"
        | "hy견명조"
        | "hymyeongjo-extra"
        | "한컴바탕" => MYEONGJO,
        // 고정폭(글자 칸 고정) 이름 → NanumGothicCoding.
        "굴림체" | "gulimche" | "돋움체" | "dotumche" | "바탕체" | "batangche" | "궁서체" | "gungsuhche" => CODING,
        // 제목용 굵은 고딕 → Black Han Sans.
        "hy헤드라인m" | "hyheadline-medium" | "hy견고딕" | "hygothic-extra" => HEADLINE,
        _ => return None,
    })
}

/// Substitutes to try, in order, for a family that isn't installed. Open fonts that are
/// metric-compatible come first (Carlito ↔ Calibri, Caladea ↔ Cambria, Liberation ↔ Arial /
/// Times New Roman / Courier New), then common system fonts, then our bundled families.
#[cfg_attr(feature = "ppm-host", allow(dead_code))] // the PPM build uses few of the chains below
fn substitutes(family: &str) -> &'static [&'static str] {
    const SANS: &[&str] = &["Carlito", "Helvetica Neue", "Arial", "Liberation Sans", "Noto Sans", "DejaVu Sans", "Source Sans 3"];
    const SERIF: &[&str] = &["Caladea", "Times New Roman", "Liberation Serif", "Georgia", "Noto Serif", "DejaVu Serif", "Source Serif 4"];
    const MONO: &[&str] = &["Courier New", "Liberation Mono", "Menlo", "Consolas", "DejaVu Sans Mono", "JetBrains Mono"];
    const ARIAL: &[&str] = &["Liberation Sans", "Helvetica", "Helvetica Neue", "Arimo", "Source Sans 3"];
    const ARIAL_UNICODE: &[&str] = &["Arial", "Liberation Sans", "Helvetica", "Helvetica Neue", "Arimo", "Source Sans 3"];
    const TIMES: &[&str] = &["Liberation Serif", "Times", "Tinos", "Source Serif 4"];
    // Word's symbol fonts (Symbol, Wingdings…) are proprietary: their characters are drawn as their
    // Unicode equivalents (`wordcraft_doc::symbol_fonts`) from these faces or any face that has them.
    const SYMBOL: &[&str] = &["Apple Symbols", "Segoe UI Symbol", "DejaVu Sans", "Noto Sans Symbols", "Noto Sans Symbols 2", "Source Sans 3"];
    // Persian / Arabic document fonts. Naskh (book) faces such as B Nazanin, B Lotus, B Zar and
    // Traditional Arabic, then the sans faces (B Yekan, B Titr, IRANSans) people use for
    // headings and screens. Open fonts with full Persian coverage first.
    const NASKH: &[&str] = &["Noto Naskh Arabic", "XB Zar", "XB Niloofar", "Amiri", "Vazirmatn", "Noto Sans Arabic", "DejaVu Sans", "Source Serif 4"];
    const ARABIC_SANS: &[&str] = &["Vazirmatn", "Sahel", "Shabnam", "Samim", "Noto Sans Arabic", "Noto Naskh Arabic", "DejaVu Sans", "Source Sans 3"];
    // Hebrew document fonts: platform-specific fonts first, then common open source fallbacks.
    // Windows: Microsoft Office fonts (David, Miriam) are common; Segoe UI is standard.
    // macOS: Helvetica has good Hebrew support; Geeza Pro is included by default.
    // Linux: Noto Sans Hebrew is the best option.
    #[cfg(windows)]
    const HEBREW: &[&str] = &["Noto Sans Hebrew", "David", "Miriam", "Segoe UI", "Arial", "Tahoma", "DejaVu Sans", "Source Sans 3"];
    #[cfg(target_os = "macos")]
    const HEBREW: &[&str] = &["Helvetica", "Noto Sans Hebrew", "Arial", "Tahoma", "Geeza Pro", "DejaVu Sans", "Source Sans 3"];
    #[cfg(not(any(windows, target_os = "macos")))]
    const HEBREW: &[&str] = &["Noto Sans Hebrew", "Segoe UI", "Arial", "Tahoma", "DejaVu Sans", "Source Sans 3"];
    // Thai document fonts (TH Sarabun New, Cordia New, the UPC families…): Thai faces, every one
    // with Thai letters; [`substitute`] prefers one with Latin letters and digits too, so a run
    // mixing Thai and Latin keeps one font at one size, as Word substitutes Cordia New for the
    // whole run (#1549). Windows' own Thai fonts first, then the Mac's, then the open Noto and Thai
    // Linux Working Group families. The Mac's Thonburi and Sathu carry Apple's `morx` tables
    // instead of OpenType ones; the shaper applies those, so their marks stack as they should.
    #[cfg(windows)]
    const THAI: &[&str] = &["Cordia New", "Leelawadee UI", "Leelawadee", "Tahoma", "Noto Sans Thai", "Browallia New", "Angsana New"];
    #[cfg(target_os = "macos")]
    const THAI: &[&str] = &["Thonburi", "Ayuthaya", "Sathu", "Krungthep", "Noto Sans Thai", "Tahoma"];
    #[cfg(not(any(windows, target_os = "macos")))]
    const THAI: &[&str] = &["Noto Sans Thai", "Laksaman", "Garuda", "Waree", "Tlwg Typo", "Loma", "Umpush", "Noto Serif Thai"];
    // Serif Thai document fonts (Angsana New, EucrosiaUPC, Norasi…) prefer a serif Thai face.
    #[cfg(windows)]
    const THAI_SERIF: &[&str] = &["Angsana New", "AngsanaUPC", "Noto Serif Thai", "Cordia New", "Leelawadee UI", "Tahoma", "Noto Sans Thai"];
    #[cfg(target_os = "macos")]
    const THAI_SERIF: &[&str] = &["Sathu", "Noto Serif Thai", "Thonburi", "Ayuthaya", "Krungthep", "Noto Sans Thai", "Tahoma"];
    #[cfg(not(any(windows, target_os = "macos")))]
    const THAI_SERIF: &[&str] = &["Noto Serif Thai", "Norasi", "Kinnari", "Noto Sans Thai", "Laksaman", "Garuda", "Waree"];
    // Palatino Linotype / Book Antiqua: macOS ships the same design as "Palatino"; open
    // look-alikes with matching metrics (TeX Gyre Pagella, URW P052 / Palladio) on Linux.
    const PALATINO: &[&str] = &[
        "Palatino Linotype",
        "Palatino",
        "Book Antiqua",
        "TeX Gyre Pagella",
        "P052",
        "URW Palladio L",
        "Palladio",
        "Caladea",
        "Times New Roman",
        "Liberation Serif",
        "Georgia",
        "Noto Serif",
        "DejaVu Serif",
        "Source Serif 4",
    ];
    let f = family.to_ascii_lowercase();
    if is_thai_serif(&f) {
        return THAI_SERIF;
    }
    if is_thai(&f) {
        return THAI;
    }
    if is_hebrew(&f) {
        return HEBREW;
    }
    if is_persian_naskh(&f) {
        return NASKH;
    }
    if is_persian_sans(&f) {
        return ARABIC_SANS;
    }
    // The PPM build closes the list: a name outside it is Carlito (no serif / mono / code guesses),
    // except the symbol fonts, whose characters need a face that has them.
    #[cfg(feature = "ppm-host")]
    {
        if let Some(chain) = ppm_office_chain(&f) {
            return chain;
        }
        return match f.as_str() {
            "symbol" | "wingdings" | "wingdings 2" | "wingdings 3" | "segoe ui symbol" | "webdings" => SYMBOL,
            _ => PPM_CARLITO,
        };
    }
    #[cfg(not(feature = "ppm-host"))]
    match f.as_str() {
        "palatino linotype" | "book antiqua" | "palatino" => PALATINO,
        "arial" | "helvetica" | "arial nova" => ARIAL,
        "arial unicode ms" => ARIAL_UNICODE,
        "times new roman" | "times" => TIMES,
        "courier new" | "courier" | "consolas" | "lucida console" | "cascadia code" | "cascadia mono" => MONO,
        "symbol" | "wingdings" | "wingdings 2" | "wingdings 3" | "segoe ui symbol" | "webdings" => SYMBOL,
        "cambria"
        | "cambria math"
        | "georgia"
        | "garamond"
        | "constantia"
        | "baskerville old face"
        | "century"
        | "century schoolbook"
        | "bookman old style"
        | "sitka text"
        | "sitka"
        | "aptos serif" => SERIF,
        _ if f.contains("mono") || f.contains("code") => MONO,
        _ if f.contains("serif") && !f.contains("sans") => SERIF,
        _ => SANS,
    }
}

/// The font a Windows 3.1-era charset name stands for: `Times New Roman Cyr`, `Arial CE`,
/// `Courier New Baltic`, `Arial (Hebrew)`… are aliases Windows keeps (its FontSubstitutes) for
/// the font itself, and old Russian and Central European documents name them. Other names are
/// returned unchanged.
pub fn charset_base_family(family: &str) -> &str {
    const SUFFIXES: &[&str] = &[" cyr", " ce", " greek", " tur", " baltic", " (arabic)", " (hebrew)", " (vietnamese)"];
    let lower = family.trim_end().to_ascii_lowercase();
    SUFFIXES
        .iter()
        .find(|s| lower.ends_with(*s) && lower.len() > s.len())
        .and_then(|s| family.trim_end().get(..lower.len() - s.len()))
        .map_or(family, str::trim_end)
}

/// Naskh-style Persian/Arabic document fonts (the "B …" family names come from the common
/// Persian font packs; matched by name only).
fn is_persian_naskh(f: &str) -> bool {
    const NAMES: &[&str] = &[
        "b nazanin",
        "b lotus",
        "b zar",
        "b mitra",
        "b badr",
        "b yagut",
        "b roya",
        "b compset",
        "b nazanin bold",
        "b lotus bold",
        "nazanin",
        "lotus",
        "zar",
        "mitra",
        "badr",
        "yagut",
        "roya",
        "ir nazanin",
        "ir lotus",
        "ir zar",
        "ir mitra",
        "ir badr",
        "irnazanin",
        "irlotus",
        "irzar",
        "irmitra",
        "irbadr",
        "traditional arabic",
        "simplified arabic",
        "arabic typesetting",
        "sakkal majalla",
        "adobe arabic",
        "andalus",
        "times new roman (arabic)",
    ];
    NAMES.contains(&f)
}

/// Sans Persian/Arabic fonts for headings and screens.
fn is_persian_sans(f: &str) -> bool {
    const NAMES: &[&str] = &[
        "b yekan",
        "b titr",
        "b homa",
        "b koodak",
        "b traffic",
        "b jadid",
        "b elham",
        "b kamran",
        "b davat",
        "b ferdosi",
        "yekan",
        "titr",
        "homa",
        "koodak",
        "traffic",
        "ir titr",
        "ir homa",
        "irtitr",
        "irhoma",
        "iransans",
        "iransansweb",
        "iran sans",
        "iranyekan",
        "iran yekan",
        "iranyekanweb",
        "dana",
        "estedad",
        "dubai",
        "aldhabi",
        "urdu typesetting",
        "arabic transparent",
    ];
    NAMES.contains(&f) || f.starts_with("iransans") || f.starts_with("iranyekan")
}

/// Thai document fonts: Word's Thai families (Cordia New, Angsana New, Browallia New, the UPC
/// families, Leelawadee), the Thai national fonts (TH Sarabun New and the other "TH …" fonts),
/// the Mac's, the Thai Linux Working Group's and Noto's. Matched by name only.
fn is_thai(f: &str) -> bool {
    const NAMES: &[&str] = &[
        "cordia new",
        "cordiaupc",
        "angsana new",
        "angsanaupc",
        "browallia new",
        "browalliaupc",
        "leelawadee",
        "leelawadee ui",
        "leelawadee ui semilight",
        "th sarabun new",
        "th sarabunpsk",
        "th sarabun psk",
        "th sarabun",
        "thonburi",
        "ayuthaya",
        "sathu",
        "krungthep",
        "silom",
        "sukhumvit set",
        "norasi",
        "kinnari",
        "garuda",
        "laksaman",
        "loma",
        "purisa",
        "sawasdee",
        "umpush",
        "waree",
        "tlwg typo",
        "tlwg typist",
        "tlwg mono",
    ];
    NAMES.contains(&f) || f.ends_with("upc") || f.starts_with("th ") || f.contains("thai")
}

/// Serif Thai document fonts.
fn is_thai_serif(f: &str) -> bool {
    const NAMES: &[&str] = &["angsana new", "angsanaupc", "eucrosiaupc", "jasmineupc", "kodchiangupc", "dilleniaupc", "norasi", "kinnari"];
    NAMES.contains(&f) || (f.contains("thai") && f.contains("serif"))
}

/// Is `family` a Thai document font ([`is_thai`])? A missing one is drawn in a Thai face for the
/// whole run, never in a Latin substitute (#1549).
pub fn is_thai_family(family: &str) -> bool {
    let f = family.to_ascii_lowercase();
    is_thai(&f) || is_thai_serif(&f)
}

/// Hebrew document fonts.
fn is_hebrew(f: &str) -> bool {
    const NAMES: &[&str] = &["david", "miriam", "noto sans hebrew", "arial hebrew", "calibri hebrew"];
    NAMES.contains(&f) || f.contains("hebrew")
}

/// The first installed substitute for a missing `family`. For a Thai font, the first with Latin
/// letters and digits as well as Thai ones (a Thai face without Latin would leave a mixed run in
/// two fonts, #1549), else the first with Thai letters, else the first installed.
fn substitute(db: &FontDb, family: &str) -> Option<&'static str> {
    let mut installed = substitutes(family).iter().copied().filter(|s| db.has_family(s));
    if !is_thai_family(family) {
        return installed.next();
    }
    let installed: Vec<&'static str> = installed.collect();
    let covers = |name: &str, chars: &[char]| {
        let face = db.face(name, "Regular");
        face.named(name) && chars.iter().all(|c| face.covers(*c))
    };
    installed
        .iter()
        .copied()
        .find(|s| covers(s, &['\u{0E19}', 'A', 'z', '0', '9']))
        .or_else(|| installed.iter().copied().find(|s| covers(s, &['\u{0E19}'])))
        .or_else(|| installed.first().copied())
}

fn style_name(bold: bool, italic: bool) -> &'static str {
    match (bold, italic) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (true, true) => "Bold Italic",
    }
}

type Memo = (u64, HashMap<(String, bool, bool), Resolved>);

fn resolve_cache() -> &'static Mutex<Memo> {
    static CACHE: OnceLock<Mutex<Memo>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new((0, HashMap::new())))
}

const FAMILY_CACHE_MAX: usize = 4096;
/// Family names longer than this (bytes) are resolved but never remembered.
const FAMILY_KEY_MAX: usize = 256;

/// Resolve `family` with bold/italic to a face (memoised until user fonts are added or removed,
/// [`FontDb::generation`]).
pub fn resolve(family: &str, bold: bool, italic: bool) -> Resolved {
    let cache = resolve_cache();
    let db = FontDb::global();
    let generation = db.generation();
    let cacheable = family.len() <= FAMILY_KEY_MAX;
    let key = (if cacheable { family.to_string() } else { String::new() }, bold, italic);
    {
        let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
        if c.0 != generation {
            *c = (generation, HashMap::new());
        }
        if cacheable && let Some(r) = c.1.get(&key) {
            return *r;
        }
    }
    // Printer-driver Symbol aliases use the same encoding as Symbol.
    let base = if family.trim().eq_ignore_ascii_case("Symbol (PCL6)") { "Symbol" } else { charset_base_family(family) };
    let (fam, substituted) = if db.has_family(family) {
        (family.to_string(), false)
    } else if base != family && db.has_family(base) {
        // Windows maps the charset names to the font itself: not a substitution.
        (base.to_string(), false)
    } else {
        (substitute(db, base).unwrap_or(crate::FALLBACK_FAMILY).to_string(), true)
    };
    let face = db.face(&fam, style_name(bold, italic));
    let r = Resolved { synth_bold: bold && face.weight < 600.0, synth_italic: italic && !face.italic, substituted, face: FaceRef::of(&face) };
    let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
    if cacheable && c.0 == generation {
        if c.1.len() >= FAMILY_CACHE_MAX {
            c.1.clear();
        }
        c.1.insert(key, r);
    }
    r
}

/// A line's (ascent, descent) from hhea values, both positive: the line gap goes above the ascent.
fn hhea_line(ascender: i16, descender: i16, line_gap: i16) -> (f64, f64) {
    (f64::from(ascender) + f64::from(line_gap.max(0)), f64::from(descender).abs())
}

/// Line metrics for a face in font units: (ascent, descent), both positive, the way word
/// processors measure single line spacing: the hhea values with the line gap above the ascent
/// (Word puts the first baseline that far below the line top), falling back to OS/2
/// usWinAscent/usWinDescent.
pub fn line_metrics(face: &crate::FontFace) -> (f64, f64) {
    static CACHE: OnceLock<Mutex<HashMap<u32, (f64, f64)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&face.id()) {
        return *v;
    }
    let mut v = (face.ascent.max(1.0), face.descent.abs());
    if let Some(f) = face.skrifa() {
        // hhea ascender + descender + line gap (Word on macOS), the gap above the ascent; OS/2
        // win metrics only when the hhea values are missing, and never more than twice the em.
        let hhea = f.hhea().ok().map(|h| hhea_line(h.ascender().to_i16(), h.descender().to_i16(), h.line_gap().to_i16()));
        let win = f.os2().ok().map(|o| (o.us_win_ascent() as f64, o.us_win_descent() as f64));
        let ok = |m: &(f64, f64)| m.0 > 0.0 && m.0 + m.1 <= face.upem * 2.0;
        if let Some(m) = hhea.filter(ok) {
            v = m;
        } else if let Some(m) = win.filter(ok) {
            v = m;
        }
    }
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(face.id(), v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_gap_goes_above_the_ascent() {
        // Times New Roman (hhea 1825 / -443, gap 87), and a font with a large gap: Word puts the
        // first baseline the ascent plus the whole gap below the line top.
        assert_eq!(hhea_line(1825, -443, 87), (1912.0, 443.0));
        assert_eq!(hhea_line(1536, -512, 452), (1988.0, 512.0));
        // A negative gap adds nothing; the most negative descender doesn't overflow.
        assert_eq!(hhea_line(1000, -300, -50), (1000.0, 300.0));
        assert_eq!(hhea_line(1000, i16::MIN, 0), (1000.0, 32768.0));
    }

    /// The PPM name mapping: the exact list (`font_mapping_canonical.md`), case-insensitive; any
    /// other name is Carlito.
    #[cfg(feature = "ppm-host")]
    #[test]
    fn office_names_map_to_the_office_compatible_set() {
        let first = |f: &str| substitutes(f)[0];
        for f in [
            "Calibri",
            "calibri light",
            "Aptos",
            "Aptos Display",
            "Aptos Narrow",
            "Aptos Light",
            "Aptos SemiBold",
            "Segoe UI",
            "Segoe UI Light",
            "Segoe UI Semibold",
            "Segoe UI Semilight",
            "Some Unknown Font",
            "Arial Black",
        ] {
            assert_eq!(first(f), "Carlito", "{f}");
        }
        for f in ["Cambria", "Constantia", "Palatino", "Palatino Linotype", "Book Antiqua", "Georgia", "Garamond", "GEORGIA"] {
            assert_eq!(first(f), "Caladea", "{f}");
        }
        for f in ["Arial", "ARIAL", "Arial Narrow", "Helvetica", "Helvetica Neue"] {
            assert_eq!(first(f), "Liberation Sans", "{f}");
        }
        for f in ["Times New Roman", "Times"] {
            assert_eq!(first(f), "Liberation Serif", "{f}");
        }
        for f in ["Courier New", "Consolas", "Courier", "Lucida Console"] {
            assert_eq!(first(f), "Liberation Mono", "{f}");
        }
        assert_eq!(first("Inter"), "Inter");
        // The list is closed: guesses by keyword or family are not made in the PPM build.
        for f in [
            "Century",
            "Century Schoolbook",
            "Aptos Serif",
            "Arial Nova",
            "Arial Unicode MS",
            "Cascadia Code",
            "Some Mono",
            "Fancy Serif",
            "Sitka Text",
            "Cambria Math",
        ] {
            assert_eq!(first(f), "Carlito", "{f}");
            assert!(!substitutes(f).contains(&"Courier New") && !substitutes(f).contains(&"Times New Roman"), "{f}");
        }
        // Symbol fonts and the names of other scripts keep their own chains.
        for f in ["Symbol", "Wingdings", "Webdings", "Segoe UI Symbol"] {
            assert!(substitutes(f).contains(&"Noto Sans Symbols 2"), "{f}");
        }
        for f in ["Cordia New", "David", "B Yekan", "B Nazanin"] {
            assert_ne!(first(f), "Carlito", "{f}");
        }
        let chain = |names: &[&str], head: &[&str]| {
            for f in names {
                let c = substitutes(f);
                assert_eq!(&c[..head.len()], head, "{f}");
                // 첫 글꼴이 없어도 한글은 NanumGothic이 받고, 라틴 대체가 이어진다.
                assert!(c.contains(&"NanumGothic") && c.contains(&"Carlito"), "{f}");
            }
        };
        chain(
            &[
                "맑은 고딕",
                "Malgun Gothic",
                "굴림",
                "Gulim",
                "새굴림",
                "New Gulim",
                "돋움",
                "Dotum",
                "함초롬돋움",
                "HCR Dotum",
                "나눔고딕",
                "NanumGothic",
                "HY중고딕",
                "HYGothic-Medium",
                "Noto Sans KR",
                "Noto Sans CJK KR",
                "Noto Serif KR",
                "Apple SD Gothic Neo",
            ],
            &["NanumGothic"],
        );
        chain(
            &[
                "바탕",
                "Batang",
                "궁서",
                "Gungsuh",
                "함초롬바탕",
                "HCR Batang",
                "나눔명조",
                "NanumMyeongjo",
                "HY신명조",
                "HYSinMyeongJo-Medium",
                "HY견명조",
                "HYMyeongJo-Extra",
                "한컴바탕",
            ],
            &["NanumMyeongjo", "NanumGothic"],
        );
        chain(&["굴림체", "GulimChe", "돋움체", "DotumChe", "바탕체", "BatangChe", "궁서체", "GungsuhChe"], &["NanumGothicCoding", "NanumGothic"]);
        chain(&["HY헤드라인M", "HYHeadLine-Medium", "HY견고딕", "HYGothic-Extra"], &["Black Han Sans", "NanumGothic"]);
    }

    /// Other builds keep the old chains: the Korean names are not special there.
    #[cfg(not(feature = "ppm-host"))]
    #[test]
    fn non_ppm_builds_keep_the_old_chains() {
        assert!(!substitutes("맑은 고딕").contains(&"NanumGothic"));
        assert_eq!(substitutes("Arial")[0], "Liberation Sans");
        assert_eq!(substitutes("Segoe UI")[0], "Carlito");
    }

    /// The PPM font set (`CRAFT_FONTS_DIR=ppm/fonts-ko`) serves the document fonts. A `ppm-host` build
    /// with the set configured must have it; only a build without the directory skips.
    #[cfg(feature = "ppm-host")]
    #[test]
    fn bundled_office_set_serves_the_document_fonts() {
        let db = FontDb::with_font_dirs(vec![]);
        if option_env!("CRAFT_FONTS_DIR").is_none() {
            eprintln!("skipped: CRAFT_FONTS_DIR was not set at build time");
            return;
        }
        let want = [
            ("Calibri", "Carlito"),
            ("Aptos Narrow", "Carlito"),
            ("Segoe UI", "Carlito"),
            ("Unknown Sans", "Carlito"),
            ("Cambria", "Caladea"),
            ("Arial", "Liberation Sans"),
            ("Times New Roman", "Liberation Serif"),
            ("Consolas", "Liberation Mono"),
            ("맑은 고딕", "NanumGothic"),
            ("굴림체", "NanumGothicCoding"),
            ("나눔고딕", "NanumGothic"),
            ("바탕", "NanumMyeongjo"),
            ("HY헤드라인M", "Black Han Sans"),
        ];
        for (name, family) in want {
            assert!(db.has_family(family), "{family} is not bundled");
            assert_eq!(substitute(&db, name), Some(family), "{name}");
        }
        // 한글 글꼴 세트: 글꼴마다 Regular(Black Han Sans는 그것만)가 읽히고, 한글 '가'를 그린다.
        for (family, styles) in [
            ("NanumGothic", &["Regular", "Bold"][..]),
            ("NanumMyeongjo", &["Regular", "Bold"]),
            ("NanumGothicCoding", &["Regular", "Bold"]),
            ("Pretendard", &["Regular", "Bold"]),
            ("Black Han Sans", &["Regular"]),
        ] {
            assert!(db.has_family(family), "{family} is not bundled");
            for style in styles {
                let f = db.face(family, style);
                assert_eq!((f.family.as_str(), f.style.as_str()), (family, *style));
                assert!(f.covers('가'), "{family} {style} draws Hangul");
            }
        }
        for family in ["Carlito", "Caladea", "Liberation Sans", "Liberation Serif", "Liberation Mono"] {
            for (style, bold, italic) in [("Regular", false, false), ("Bold", true, false), ("Italic", false, true), ("Bold Italic", true, true)] {
                let f = db.face(family, style);
                assert_eq!(f.family, family, "{family} {style}");
                assert_eq!((f.weight >= 600.0, f.italic), (bold, italic), "{family} {style}");
            }
        }
    }

    /// The menu's Hangul test reads the font files: Latin-only bundled families are hidden, the Korean
    /// set is not (the latter only in a build with the PPM font set).
    #[test]
    fn bundled_families_without_hangul_are_hidden_from_the_menu() {
        let db = FontDb::with_font_dirs(vec![]);
        let hide = db.bundled_without_hangul();
        for f in ["source sans 3", "inter"] {
            assert!(hide.contains(f), "{f}");
        }
        let menu = db.families_in_except("en", &hide);
        assert!(!menu.iter().any(|f| f.eq_ignore_ascii_case("Source Sans 3")));
        assert!(db.families().iter().any(|f| f.eq_ignore_ascii_case("Source Sans 3")), "still loaded");
        if db.has_family("NanumGothic") {
            for f in ["nanumgothic", "nanummyeongjo", "nanumgothiccoding", "black han sans", "pretendard"] {
                assert!(!hide.contains(f) && menu.iter().any(|m| m.eq_ignore_ascii_case(f)), "{f} is listed");
            }
        }
    }

    #[test]
    fn family_cache_stays_bounded() {
        for i in 0..(FAMILY_CACHE_MAX + 50) {
            let _ = resolve(&format!("Zz Missing Family {i}"), false, false);
        }
        assert!(resolve_cache().lock().unwrap_or_else(|e| e.into_inner()).1.len() <= FAMILY_CACHE_MAX);
        let long = "L".repeat(10_000);
        let _ = resolve(&long, false, false);
        assert!(!resolve_cache().lock().unwrap_or_else(|e| e.into_inner()).1.keys().any(|k| k.0.len() > 1000));
    }

    #[test]
    fn unknown_family_substitutes() {
        let r = resolve("Definitely Not A Font 123", false, false);
        assert!(r.substituted);
        let b = resolve("Source Serif 4", true, false);
        assert!(!b.substituted);
        assert!(!b.synth_bold);
        let (a, d) = line_metrics(&b.face);
        assert!(a > 0.0 && d >= 0.0);
    }

    // The PPM build maps Palatino onto Caladea instead (`office_names_map_to_the_office_compatible_set`).
    #[cfg(not(feature = "ppm-host"))]
    #[test]
    fn palatino_family_prefers_palatino_look_alikes_over_generic_serif() {
        for name in ["Palatino Linotype", "Book Antiqua", "palatino"] {
            let list = substitutes(name);
            let pos = |n: &str| list.iter().position(|x| *x == n);
            assert!(pos("Palatino").is_some() && pos("TeX Gyre Pagella").is_some() && pos("P052").is_some(), "{name}");
            // All of them come before the generic serif fallbacks (Caladea, Times New Roman).
            assert!(pos("Palatino") < pos("Caladea") && pos("TeX Gyre Pagella") < pos("Times New Roman"), "{name}");
        }
        // Other serif families are unchanged.
        assert_eq!(substitutes("Georgia").first(), Some(&"Caladea"));
        // Charset aliases are the font itself, so they substitute like it.
        assert_eq!(charset_base_family("Times New Roman Cyr"), "Times New Roman");
        assert_eq!(charset_base_family("Arial CYR"), "Arial");
        assert_eq!(charset_base_family("Courier New CE"), "Courier New");
        assert_eq!(charset_base_family("Arial (Hebrew)"), "Arial");
        assert_eq!(charset_base_family("Cyr"), "Cyr");
        assert_eq!(charset_base_family("Calibri"), "Calibri");
        assert_eq!(substitutes(charset_base_family("Times New Roman Cyr")), substitutes("Times New Roman"));
    }

    #[test]
    fn pcl6_symbol_resolves_as_symbol() {
        let actual = resolve("Symbol (PCL6)", false, false);
        let db = FontDb::global();
        if db.has_family("Symbol (PCL6)") {
            assert_eq!(actual.face.id(), db.face("Symbol (PCL6)", "Regular").id());
            assert!(!actual.substituted);
            return;
        }
        let expected = resolve("Symbol", false, false);
        assert_eq!(actual.face.id(), expected.face.id());
        assert_eq!(actual.substituted, expected.substituted);
    }

    #[cfg(not(feature = "ppm-host"))]
    #[test]
    fn unicode_arial_is_proportional() {
        for name in ["Arial Unicode MS", "ARIAL UNICODE MS"] {
            let candidates = substitutes(name);
            assert_eq!(candidates.first(), Some(&"Arial"));
            assert_eq!(&candidates[1..], substitutes("Arial"));
        }
        assert_eq!(substitutes("Cascadia Code").first(), Some(&"Courier New"));
    }

    #[test]
    fn symbol_fonts_substitute_symbol_faces() {
        for f in ["Symbol", "Wingdings", "Wingdings 2", "Wingdings 3", "Webdings"] {
            assert!(substitutes(f).contains(&"Noto Sans Symbols 2"), "{f}");
        }
    }

    #[test]
    fn resolves_weights_named_as_their_own_family() {
        // Word stores a Medium face as the family "Inter Medium" (#219).
        let r = resolve("Inter Medium", false, false);
        assert!(!r.substituted && !r.synth_bold && !r.synth_italic);
        assert_eq!((r.face.family.as_str(), r.face.style.as_str()), ("Inter", "Medium"));
        // Bold and italic within it are drawn from the Medium face.
        let b = resolve("Inter Medium", true, true);
        assert_eq!(b.face.style, "Medium");
        assert!(!b.substituted && b.synth_bold && b.synth_italic);
    }

    #[test]
    fn synthesizes_missing_styles() {
        // JetBrains Mono ships only Regular here.
        let r = resolve("JetBrains Mono", true, true);
        assert!(r.synth_bold && r.synth_italic);
    }

    #[test]
    fn hebrew_fonts_substitute_hebrew_faces() {
        for f in ["David", "Miriam", "Arial Hebrew", "Calibri Hebrew", "Noto Sans Hebrew"] {
            assert!(substitutes(f).contains(&"Noto Sans Hebrew"), "{f}");
            // David is a Windows font; only the Windows list offers it.
            if cfg!(target_os = "windows") {
                assert!(substitutes(f).contains(&"David") || f == "David", "{f}");
            }
        }
        // Generic names with "Hebrew" in them also get Hebrew substitutions
        assert!(substitutes("SomeFont Hebrew").contains(&"Noto Sans Hebrew"));
    }

    #[test]
    fn thai_fonts_substitute_thai_faces() {
        // Every Thai document family maps to a list of Thai faces, never to the Latin lists.
        for f in [
            "TH Sarabun New",
            "TH SarabunPSK",
            "Angsana New",
            "AngsanaUPC",
            "Cordia New",
            "CordiaUPC",
            "Browallia New",
            "BrowalliaUPC",
            "JasmineUPC",
            "KodchiangUPC",
            "LilyUPC",
            "DilleniaUPC",
            "EucrosiaUPC",
            "FreesiaUPC",
            "IrisUPC",
            "Leelawadee",
            "Leelawadee UI",
            "Noto Sans Thai",
            "TH Niramit AS",
        ] {
            assert!(is_thai_family(f), "{f}");
            let list = substitutes(f);
            assert!(list.iter().any(|s| s.contains("Thai")), "{f}: {list:?}");
            assert!(!list.contains(&"Carlito") && !list.contains(&"Caladea"), "{f}: {list:?}");
        }
        // Serif Thai families prefer a serif Thai face; the sans families a sans one.
        let pos = |family: &str, name: &str| substitutes(family).iter().position(|s| *s == name).unwrap_or(usize::MAX);
        assert!(pos("Angsana New", "Noto Serif Thai") < pos("Angsana New", "Noto Sans Thai"));
        assert!(pos("TH Sarabun New", "Noto Sans Thai") < pos("TH Sarabun New", "Noto Serif Thai"));
        // Latin and other script families are unchanged.
        assert!(!is_thai_family("Arial") && !is_thai_family("Tahoma") && !is_thai_family("Calibri") && !is_thai_family("David"));
        assert_eq!(substitutes("Calibri").first(), Some(&"Carlito"));
    }

    #[test]
    fn a_missing_thai_font_resolves_to_an_installed_thai_face() {
        // The issue's run (#1549): Thai letters and digits, Latin letters, digits and the baht
        // sign in one run of a Thai font that isn't installed.
        const RUN: &str = "[D] น้ำ ค่ำ ปั้น ฝั่ง ปู่ ๑๒๓๔๕๖๗๘๙๐ ฿1,250.50";
        let db = FontDb::global();
        for family in ["TH Sarabun New", "Angsana New", "Cordia New", "BrowalliaUPC"] {
            if db.has_family(family) {
                continue;
            }
            let r = resolve(family, false, false);
            let face = r.face.get();
            if !face.covers('\u{0E19}') {
                eprintln!("skipped {family}: no Thai face from its substitute list is installed");
                continue;
            }
            assert!(r.substituted, "{family}");
            let thai: Vec<char> = RUN.chars().filter(|c| ('\u{0E00}'..='\u{0E7F}').contains(c) && !face.covers(*c)).collect();
            assert!(thai.is_empty(), "{family} → {} lacks {thai:?}", face.family);
            // A listed Thai face with Latin letters and digits wins over one without them.
            let latin = |name: &str| {
                let f = db.face(name, "Regular");
                f.named(name) && f.covers('\u{0E19}') && f.covers('D') && f.covers('5')
            };
            if substitutes(family).iter().any(|s| db.has_family(s) && latin(s)) {
                let missing: Vec<char> = RUN.chars().filter(|c| !c.is_whitespace() && !face.covers(*c)).collect();
                assert!(missing.is_empty(), "{family} → {} lacks {missing:?}", face.family);
            }
        }
    }

    #[test]
    fn thonburi_stacks_thai_marks_through_its_apple_tables() {
        // Thonburi has `morx` but no GSUB: the shaper must still move a tone mark up over an upper
        // vowel (ปั่) instead of drawing it where it sits on a bare consonant (ป่).
        let db = FontDb::global();
        if !db.has_family("Thonburi") {
            eprintln!("skipped: Thonburi not installed");
            return;
        }
        let face = db.face("Thonburi", "Regular");
        let tone = |text: &str| crate::shape(&face, text, &[], |c| c).into_iter().last().map(|g| (g.gid, g.y_offset));
        let (bare, stacked) = (tone("\u{0E1B}\u{0E48}"), tone("\u{0E1B}\u{0E31}\u{0E48}"));
        assert!(bare.is_some() && stacked.is_some());
        assert_ne!(bare, stacked, "the tone mark over ั is raised or swapped");
    }
}
