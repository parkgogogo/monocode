use crate::{Font, FontWeight, SharedString};

/// A virtual font family that the application can retarget at runtime.
///
/// Elements name the alias (for example `.AppUI`) instead of a concrete
/// family. [`crate::TextSystem::set_font_aliases`] decides which installed
/// family it resolves to, how its weights shift, and how its sizes scale, so
/// changing a font preference never requires touching individual call sites.
#[derive(Clone, Debug, PartialEq)]
pub struct FontAlias {
    /// The concrete family this alias resolves to.
    pub family: SharedString,
    /// The family used when `family` cannot be loaded, such as after the
    /// user uninstalls it.
    pub fallback: SharedString,
    /// Added to every requested weight, so bold text stays relatively bolder
    /// when the base weight changes.
    pub weight_offset: f32,
    /// Multiplies absolute font sizes and line heights in text styles that use
    /// this alias.
    pub size_scale: f32,
}

impl FontAlias {
    /// An alias that resolves to `family` with no weight or size adjustment.
    pub fn new(family: impl Into<SharedString>) -> Self {
        let family = family.into();
        Self {
            fallback: family.clone(),
            family,
            weight_offset: 0.,
            size_scale: 1.,
        }
    }

    /// The concrete font for a request against this alias.
    pub(crate) fn resolve(&self, font: &Font) -> Font {
        self.with_family(font, self.family.clone())
    }

    /// The concrete font to try when [`Self::resolve`] fails to load.
    pub(crate) fn fallback(&self, font: &Font) -> Option<Font> {
        (self.fallback != self.family).then(|| self.with_family(font, self.fallback.clone()))
    }

    fn with_family(&self, font: &Font, family: SharedString) -> Font {
        Font {
            family,
            weight: FontWeight((font.weight.0 + self.weight_offset).clamp(100., 900.)),
            ..font.clone()
        }
    }
}
