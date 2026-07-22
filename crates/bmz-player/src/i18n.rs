//! BMZ Player のデスクトップ UI 向け翻訳基盤。
//!
//! locale は呼び出し側が明示的に渡す。現在 locale を可変 global に保持しないため、
//! profile の切り替えやテストを互いに干渉させずに扱える。

use std::fmt;
use std::sync::LazyLock;

use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
pub use fluent_bundle::{FluentArgs, FluentValue};
use unic_langid::LanguageIdentifier;

/// BMZ Player が UI 表示に対応する locale。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AppLocale {
    #[default]
    Ja,
    En,
    Ko,
    ZhCn,
    ZhTw,
    ZhHk,
}

impl AppLocale {
    pub const DEFAULT: Self = Self::Ja;

    /// ComboBox などに表示する順序を含む対応 locale 一覧。
    pub const SUPPORTED: [Self; 6] =
        [Self::Ja, Self::En, Self::Ko, Self::ZhCn, Self::ZhTw, Self::ZhHk];

    /// profile.toml に保存する canonical language code。
    pub const fn code(self) -> &'static str {
        match self {
            Self::Ja => "ja",
            Self::En => "en",
            Self::Ko => "ko",
            Self::ZhCn => "zh-CN",
            Self::ZhTw => "zh-TW",
            Self::ZhHk => "zh-HK",
        }
    }

    /// 言語選択 UI に表示する、各言語自身での名称。
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::Ja => "日本語",
            Self::En => "English",
            Self::Ko => "한국어",
            Self::ZhCn => "简体中文",
            Self::ZhTw => "繁體中文（台灣）",
            Self::ZhHk => "繁體中文（香港）",
        }
    }

    /// 未指定フォントの地域別 CJK 字形で最優先する coverage。
    ///
    /// 英語でも曲メタデータに CJK が混在するため、日本語 face を primary にしつつ
    /// renderer 側の全 CJK fallback chain は維持する。
    pub const fn font_coverage(self) -> bmz_render::FontCoverage {
        match self {
            Self::Ja | Self::En => bmz_render::FontCoverage::Japanese,
            Self::Ko => bmz_render::FontCoverage::Korean,
            Self::ZhCn => bmz_render::FontCoverage::SimplifiedChinese,
            Self::ZhTw => bmz_render::FontCoverage::TraditionalChinese,
            Self::ZhHk => bmz_render::FontCoverage::HongKong,
        }
    }

    /// BCP 47 風の入力を対応 locale へ正規化する。
    ///
    /// 大文字小文字と `_` / `-` の違いを無視し、地域付きの ja/en/ko、
    /// zh-Hans/zh-Hant と中国語の主要地域 alias を受理する。
    pub fn from_code(code: &str) -> Option<Self> {
        let normalized = code.trim().replace('_', "-").to_ascii_lowercase();
        let primary = normalized.split('-').next()?;

        match primary {
            "ja" => Some(Self::Ja),
            "en" => Some(Self::En),
            "ko" => Some(Self::Ko),
            "zh" => {
                let subtags: Vec<_> = normalized.split('-').skip(1).collect();
                if subtags.iter().any(|part| matches!(*part, "hk" | "mo")) {
                    Some(Self::ZhHk)
                } else if subtags.iter().any(|part| matches!(*part, "tw" | "hant")) {
                    Some(Self::ZhTw)
                } else {
                    // bare zh、Hans、CN、SG は簡体字カタログへ寄せる。
                    Some(Self::ZhCn)
                }
            }
            _ => None,
        }
    }

    /// profile の String 設定値を解決する。不明値は `ja` へ安全に戻す。
    pub fn profile_language(language: &str) -> Self {
        match Self::from_code(language) {
            Some(locale) => locale,
            None => {
                tracing::warn!(
                    language,
                    fallback = Self::DEFAULT.code(),
                    "unsupported profile language; using fallback"
                );
                Self::DEFAULT
            }
        }
    }

    /// 指定 locale の message fallback 順。
    ///
    /// `ja` を必須の最終基準とし、香港向け繁体字は台湾向け繁体字も参照する。
    pub const fn fallback_chain(self) -> &'static [Self] {
        match self {
            Self::Ja => &[Self::Ja],
            Self::En => &[Self::En, Self::Ja],
            Self::Ko => &[Self::Ko, Self::En, Self::Ja],
            Self::ZhCn => &[Self::ZhCn, Self::En, Self::Ja],
            Self::ZhTw => &[Self::ZhTw, Self::En, Self::Ja],
            Self::ZhHk => &[Self::ZhHk, Self::ZhTw, Self::En, Self::Ja],
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for AppLocale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// locale を明示的に保持する軽量な翻訳 facade。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Localizer {
    locale: AppLocale,
}

impl Localizer {
    pub const fn new(locale: AppLocale) -> Self {
        Self { locale }
    }

    pub const fn locale(self) -> AppLocale {
        self.locale
    }

    pub fn text(self, key: &str) -> String {
        text(self.locale, key)
    }

    pub fn format(self, key: &str, args: &FluentArgs<'_>) -> String {
        format(self.locale, key, args)
    }
}

/// 引数を持たない message を現在 locale と fallback chain から解決する。
/// 未登録 key は、欠落が画面上で判別できるよう key 自体を返す。
pub fn text(locale: AppLocale, key: &str) -> String {
    CATALOGS.format(locale, key, None).unwrap_or_else(|| key.to_owned())
}

/// 引数付き message を現在 locale と fallback chain から解決する。
/// 未登録 key は、欠落が画面上で判別できるよう key 自体を返す。
pub fn format(locale: AppLocale, key: &str, args: &FluentArgs<'_>) -> String {
    CATALOGS.format(locale, key, Some(args)).unwrap_or_else(|| key.to_owned())
}

const CATALOG_SOURCES: [&str; 6] = [
    include_str!("i18n/locales/ja.ftl"),
    include_str!("i18n/locales/en.ftl"),
    include_str!("i18n/locales/ko.ftl"),
    include_str!("i18n/locales/zh-CN.ftl"),
    include_str!("i18n/locales/zh-TW.ftl"),
    include_str!("i18n/locales/zh-HK.ftl"),
];

type Bundle = FluentBundle<FluentResource>;

struct Catalogs {
    bundles: [Bundle; 6],
}

impl Catalogs {
    fn new() -> Self {
        Self::from_sources(CATALOG_SOURCES)
    }

    fn from_sources(sources: [&str; 6]) -> Self {
        let bundles = AppLocale::SUPPORTED.map(|locale| {
            let source = sources[locale.index()];
            let resource =
                FluentResource::try_new(source.to_owned()).unwrap_or_else(|(_, errors)| {
                    panic!("invalid {} Fluent catalog: {errors:?}", locale.code())
                });
            let language: LanguageIdentifier = locale
                .code()
                .parse()
                .unwrap_or_else(|error| panic!("invalid locale {}: {error}", locale.code()));
            let mut bundle = Bundle::new_concurrent(vec![language]);
            // BMZ の対象 locale はすべて LTR で、不可視の FSI/PDI はスキン描画の
            // 文字幅・caret byte index・検索履歴の表示文字列をずらしてしまう。
            bundle.set_use_isolating(false);
            bundle.add_resource(resource).unwrap_or_else(|errors| {
                panic!("failed to add {} Fluent catalog: {errors:?}", locale.code())
            });
            bundle
        });
        Self { bundles }
    }

    fn format(
        &self,
        locale: AppLocale,
        key: &str,
        args: Option<&FluentArgs<'_>>,
    ) -> Option<String> {
        for candidate in locale.fallback_chain() {
            let bundle = &self.bundles[candidate.index()];
            let Some(message) = bundle.get_message(key) else {
                continue;
            };
            let Some(pattern) = message.value() else {
                continue;
            };
            let mut errors = Vec::new();
            let result = bundle.format_pattern(pattern, args, &mut errors).into_owned();
            if !errors.is_empty() {
                tracing::warn!(locale = candidate.code(), key, ?errors, "failed to format message");
            }
            return Some(result);
        }
        None
    }
}

static CATALOGS: LazyLock<Catalogs> = LazyLock::new(Catalogs::new);

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use fluent_syntax::parser;

    use super::*;

    #[test]
    fn supported_locales_have_stable_canonical_codes() {
        assert_eq!(
            AppLocale::SUPPORTED.map(AppLocale::code),
            ["ja", "en", "ko", "zh-CN", "zh-TW", "zh-HK"]
        );
        assert!(AppLocale::SUPPORTED.iter().all(|locale| !locale.native_name().is_empty()));
    }

    #[test]
    fn locales_map_to_region_appropriate_font_coverage() {
        use bmz_render::FontCoverage;

        for (locale, expected) in [
            (AppLocale::Ja, FontCoverage::Japanese),
            (AppLocale::En, FontCoverage::Japanese),
            (AppLocale::Ko, FontCoverage::Korean),
            (AppLocale::ZhCn, FontCoverage::SimplifiedChinese),
            (AppLocale::ZhTw, FontCoverage::TraditionalChinese),
            (AppLocale::ZhHk, FontCoverage::HongKong),
        ] {
            assert_eq!(locale.font_coverage(), expected, "{}", locale.code());
        }
    }

    #[test]
    fn locale_aliases_are_normalized() {
        for (input, expected) in [
            ("ja-JP", AppLocale::Ja),
            ("EN_us", AppLocale::En),
            ("ko-KR", AppLocale::Ko),
            ("zh", AppLocale::ZhCn),
            ("zh-Hans", AppLocale::ZhCn),
            ("zh-SG", AppLocale::ZhCn),
            ("zh-Hant", AppLocale::ZhTw),
            ("zh_TW", AppLocale::ZhTw),
            ("zh-Hant-HK", AppLocale::ZhHk),
            ("zh-MO", AppLocale::ZhHk),
        ] {
            assert_eq!(AppLocale::from_code(input), Some(expected), "{input}");
        }
        assert_eq!(AppLocale::from_code("fr"), None);
        assert_eq!(AppLocale::profile_language("fr"), AppLocale::Ja);
    }

    #[test]
    fn fallback_chains_end_at_japanese_catalog() {
        assert_eq!(
            AppLocale::ZhHk.fallback_chain(),
            &[AppLocale::ZhHk, AppLocale::ZhTw, AppLocale::En, AppLocale::Ja]
        );
        for locale in AppLocale::SUPPORTED {
            assert_eq!(locale.fallback_chain().last(), Some(&AppLocale::Ja));
        }
    }

    #[test]
    fn missing_message_uses_fallback_catalog() {
        let catalogs = Catalogs::from_sources([
            "fallback-only = 日本語の基準値",
            "english-only = English",
            "korean-only = 한국어",
            "simplified-only = 简体中文",
            "traditional-only = 繁體中文",
            "hong-kong-only = 繁體中文（香港）",
        ]);
        assert_eq!(
            catalogs.format(AppLocale::ZhHk, "traditional-only", None).as_deref(),
            Some("繁體中文")
        );
        assert_eq!(
            catalogs.format(AppLocale::Ko, "fallback-only", None).as_deref(),
            Some("日本語の基準値")
        );
    }

    #[test]
    fn localizer_formats_arguments_without_global_locale() {
        let mut args = FluentArgs::new();
        args.set("language", AppLocale::Ko.native_name());
        assert!(
            Localizer::new(AppLocale::En)
                .format("settings-language-current", &args)
                .contains(AppLocale::Ko.native_name())
        );
        assert_eq!(Localizer::new(AppLocale::Ja).text("missing-message"), "missing-message");
    }

    #[test]
    fn catalogs_have_matching_keys_and_placeholders() {
        let expected = catalog_signature(AppLocale::Ja, CATALOG_SOURCES[AppLocale::Ja.index()]);
        for locale in AppLocale::SUPPORTED.into_iter().filter(|locale| *locale != AppLocale::Ja) {
            assert_eq!(
                catalog_signature(locale, CATALOG_SOURCES[locale.index()]),
                expected,
                "{} catalog differs from ja",
                locale.code()
            );
        }
    }

    fn catalog_signature(locale: AppLocale, source: &str) -> BTreeMap<String, BTreeSet<String>> {
        parser::parse(source)
            .unwrap_or_else(|(_, errors)| panic!("invalid {} catalog: {errors:?}", locale.code()));

        let mut messages = BTreeMap::<String, BTreeSet<String>>::new();
        let mut current_key = None;
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let value = if !line.chars().next().is_some_and(char::is_whitespace) {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim().to_owned();
                messages.entry(key.clone()).or_default();
                current_key = Some(key);
                value
            } else {
                line
            };

            if let Some(key) = &current_key {
                messages.entry(key.clone()).or_default().extend(placeholder_names(value));
            }
        }
        messages
    }

    fn placeholder_names(value: &str) -> BTreeSet<String> {
        let chars: Vec<_> = value.chars().collect();
        let mut result = BTreeSet::new();
        let mut index = 0;
        while index < chars.len() {
            if chars[index] != '$' {
                index += 1;
                continue;
            }
            index += 1;
            let start = index;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '_' | '-'))
            {
                index += 1;
            }
            if start != index {
                result.insert(chars[start..index].iter().collect());
            }
        }
        result
    }
}
