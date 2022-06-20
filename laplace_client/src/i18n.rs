use std::{collections::HashMap, sync::Arc};

use arc_swap::{ArcSwap, Guard};
use lazy_static::lazy_static;

pub type TextMap = HashMap<String, String>;

pub const DEFAULT_LANG: &'static str = "en";

lazy_static! {
    static ref CURRENT_LANG: ArcSwap<String> = ArcSwap::from_pointee(DEFAULT_LANG.to_string());
    static ref TRANSLATIONS: ArcSwap<HashMap<String, TextMap>> = ArcSwap::from_pointee(default_translations());
}

pub mod label {
    pub const SETTINGS: &'static str = "Settings";
    pub const APPLICATIONS: &'static str = "Applications";
    pub const ADD_LAPP: &'static str = "Add lapp";
}

pub fn default_translations() -> HashMap<String, TextMap> {
    [(
        DEFAULT_LANG.into(),
        [
            (label::SETTINGS.into(), "Settings".into()),
            (label::APPLICATIONS.into(), "Applications".into()),
            (label::ADD_LAPP.into(), "Add lapp".into()),
        ]
        .into(),
    )]
    .into()
}

#[inline]
pub fn load() -> I18n {
    I18n {
        current_lang: CURRENT_LANG.load(),
        translations: TRANSLATIONS.load(),
    }
}

#[inline]
pub fn switch_lang(lang: impl Into<String>) -> bool {
    let lang = lang.into();

    if TRANSLATIONS.load().contains_key(&lang) {
        CURRENT_LANG.swap(Arc::new(lang));
        true
    } else {
        false
    }
}

pub fn add_translations(translations: Vec<(String, TextMap)>) {
    TRANSLATIONS.rcu(|old_translations| {
        let mut new_translations = HashMap::clone(&old_translations);
        for (lang, text_map) in &translations {
            new_translations.insert(lang.clone(), text_map.clone());
        }
        new_translations
    });
}

pub struct I18n {
    pub current_lang: Guard<Arc<String>>,
    pub translations: Guard<Arc<HashMap<String, TextMap>>>,
}

impl I18n {
    pub fn text<'a>(&'a self, label: &'a str) -> &'a str {
        self.translate(label).unwrap_or_else(|| label)
    }

    fn translate(&self, label: &str) -> Option<&str> {
        let translations = if let Some(translations) = self.translations.get(self.current_lang.as_str()) {
            translations
        } else {
            self.translations.get(DEFAULT_LANG)?
        };
        translations.get(label).map(String::as_str)
    }
}
