use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

const BUILTIN_MORPHEME_ONTOLOGY_JSON: &str =
    include_str!("../../../ruhi_lexicon/data/morpheme_ontology.json");

/// 漢語語素語意本體分類枚舉（對應 64-bit `MorphemeSemanticMask` 位元索引 0..=43）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum MorphemeClass {
    WeatherPrecipMod = 0,
    WeatherPrecipHead = 1,
    WeatherWindMod = 2,
    WeatherWindHead = 3,
    WeatherVaporMod = 4,
    WeatherVaporHead = 5,
    WeatherSevereMod = 6,
    WeatherSevereHead = 7,
    WeatherActionVerb = 8,
    LampMod = 9,
    LampHead = 10,
    FireLightMod = 11,
    FireLightHead = 12,
    LuminousInchoativeState = 13,
    LuminousDisposalVerb = 14,
    LuminousRelClausePred = 15,
    MotionGazeVerbStem = 16,
    DirectionalPrep = 17,
    DirectionalComp1 = 18,
    DirectionalComp2 = 19,
    SpatialDimAdj = 20,
    WallPrefixMod = 21,
    BarrierHead = 22,
    BlockingVerbStem = 23,
    PotentialPolarity = 24,
    ResultStaySuffix = 25,
    DurationVerb = 26,
    DurationDegreeAdv = 27,
    BeverageContext = 28,
    SelectActionStem = 29,
    SelectActionHead = 30,
    UiContainerSuffix = 31,
    TransferVerbStem = 32,
    DepletionVerbStem = 33,
    CognitiveDomainStem = 34,
    TextualUnitMod = 35,
    DurativeAdv = 36,
    ConcessiveFocusTrigger = 37,
    WhIndefinitePronoun = 38,
    UniversalConcordOp = 39,
    DisposalPrep = 40,
    PassivePrep = 41,
    SerialMotionV1 = 42,
    BoundHeadMorpheme = 43,
    RepeatableActionVerb = 44,
}

impl MorphemeClass {
    #[inline]
    pub const fn bit(self) -> u64 {
        1u64 << (self as u32)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WeatherPrecipMod => "WeatherPrecipMod",
            Self::WeatherPrecipHead => "WeatherPrecipHead",
            Self::WeatherWindMod => "WeatherWindMod",
            Self::WeatherWindHead => "WeatherWindHead",
            Self::WeatherVaporMod => "WeatherVaporMod",
            Self::WeatherVaporHead => "WeatherVaporHead",
            Self::WeatherSevereMod => "WeatherSevereMod",
            Self::WeatherSevereHead => "WeatherSevereHead",
            Self::WeatherActionVerb => "WeatherActionVerb",
            Self::LampMod => "LampMod",
            Self::LampHead => "LampHead",
            Self::FireLightMod => "FireLightMod",
            Self::FireLightHead => "FireLightHead",
            Self::LuminousInchoativeState => "LuminousInchoativeState",
            Self::LuminousDisposalVerb => "LuminousDisposalVerb",
            Self::LuminousRelClausePred => "LuminousRelClausePred",
            Self::MotionGazeVerbStem => "MotionGazeVerbStem",
            Self::DirectionalPrep => "DirectionalPrep",
            Self::DirectionalComp1 => "DirectionalComp1",
            Self::DirectionalComp2 => "DirectionalComp2",
            Self::SpatialDimAdj => "SpatialDimAdj",
            Self::WallPrefixMod => "WallPrefixMod",
            Self::BarrierHead => "BarrierHead",
            Self::BlockingVerbStem => "BlockingVerbStem",
            Self::PotentialPolarity => "PotentialPolarity",
            Self::ResultStaySuffix => "ResultStaySuffix",
            Self::DurationVerb => "DurationVerb",
            Self::DurationDegreeAdv => "DurationDegreeAdv",
            Self::BeverageContext => "BeverageContext",
            Self::SelectActionStem => "SelectActionStem",
            Self::SelectActionHead => "SelectActionHead",
            Self::UiContainerSuffix => "UiContainerSuffix",
            Self::TransferVerbStem => "TransferVerbStem",
            Self::DepletionVerbStem => "DepletionVerbStem",
            Self::CognitiveDomainStem => "CognitiveDomainStem",
            Self::TextualUnitMod => "TextualUnitMod",
            Self::DurativeAdv => "DurativeAdv",
            Self::ConcessiveFocusTrigger => "ConcessiveFocusTrigger",
            Self::WhIndefinitePronoun => "WhIndefinitePronoun",
            Self::UniversalConcordOp => "UniversalConcordOp",
            Self::DisposalPrep => "DisposalPrep",
            Self::PassivePrep => "PassivePrep",
            Self::SerialMotionV1 => "SerialMotionV1",
            Self::BoundHeadMorpheme => "BoundHeadMorpheme",
            Self::RepeatableActionVerb => "RepeatableActionVerb",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "WeatherPrecipMod" => Some(Self::WeatherPrecipMod),
            "WeatherPrecipHead" => Some(Self::WeatherPrecipHead),
            "WeatherWindMod" => Some(Self::WeatherWindMod),
            "WeatherWindHead" => Some(Self::WeatherWindHead),
            "WeatherVaporMod" => Some(Self::WeatherVaporMod),
            "WeatherVaporHead" => Some(Self::WeatherVaporHead),
            "WeatherSevereMod" => Some(Self::WeatherSevereMod),
            "WeatherSevereHead" => Some(Self::WeatherSevereHead),
            "WeatherActionVerb" => Some(Self::WeatherActionVerb),
            "LampMod" => Some(Self::LampMod),
            "LampHead" => Some(Self::LampHead),
            "FireLightMod" => Some(Self::FireLightMod),
            "FireLightHead" => Some(Self::FireLightHead),
            "LuminousInchoativeState" => Some(Self::LuminousInchoativeState),
            "LuminousDisposalVerb" => Some(Self::LuminousDisposalVerb),
            "LuminousRelClausePred" => Some(Self::LuminousRelClausePred),
            "MotionGazeVerbStem" => Some(Self::MotionGazeVerbStem),
            "DirectionalPrep" => Some(Self::DirectionalPrep),
            "DirectionalComp1" => Some(Self::DirectionalComp1),
            "DirectionalComp2" => Some(Self::DirectionalComp2),
            "SpatialDimAdj" => Some(Self::SpatialDimAdj),
            "WallPrefixMod" => Some(Self::WallPrefixMod),
            "BarrierHead" => Some(Self::BarrierHead),
            "BlockingVerbStem" => Some(Self::BlockingVerbStem),
            "PotentialPolarity" => Some(Self::PotentialPolarity),
            "ResultStaySuffix" => Some(Self::ResultStaySuffix),
            "DurationVerb" => Some(Self::DurationVerb),
            "DurationDegreeAdv" => Some(Self::DurationDegreeAdv),
            "BeverageContext" => Some(Self::BeverageContext),
            "SelectActionStem" => Some(Self::SelectActionStem),
            "SelectActionHead" => Some(Self::SelectActionHead),
            "UiContainerSuffix" => Some(Self::UiContainerSuffix),
            "TransferVerbStem" => Some(Self::TransferVerbStem),
            "DepletionVerbStem" => Some(Self::DepletionVerbStem),
            "CognitiveDomainStem" => Some(Self::CognitiveDomainStem),
            "TextualUnitMod" => Some(Self::TextualUnitMod),
            "DurativeAdv" => Some(Self::DurativeAdv),
            "ConcessiveFocusTrigger" => Some(Self::ConcessiveFocusTrigger),
            "WhIndefinitePronoun" => Some(Self::WhIndefinitePronoun),
            "UniversalConcordOp" => Some(Self::UniversalConcordOp),
            "DisposalPrep" => Some(Self::DisposalPrep),
            "PassivePrep" => Some(Self::PassivePrep),
            "SerialMotionV1" => Some(Self::SerialMotionV1),
            "BoundHeadMorpheme" => Some(Self::BoundHeadMorpheme),
            "RepeatableActionVerb" => Some(Self::RepeatableActionVerb),
            _ => None,
        }
    }
}

pub const WEATHER_HEAD_MASK: u64 = MorphemeClass::WeatherPrecipHead.bit()
    | MorphemeClass::WeatherWindHead.bit()
    | MorphemeClass::WeatherVaporHead.bit()
    | MorphemeClass::WeatherSevereHead.bit();

pub const WEATHER_MOD_MASK: u64 = MorphemeClass::WeatherPrecipMod.bit()
    | MorphemeClass::WeatherWindMod.bit()
    | MorphemeClass::WeatherVaporMod.bit()
    | MorphemeClass::WeatherSevereMod.bit();

pub const LUMINOUS_MOD_MASK: u64 =
    MorphemeClass::LampMod.bit() | MorphemeClass::FireLightMod.bit();

pub const LUMINOUS_HEAD_MASK: u64 =
    MorphemeClass::LampHead.bit() | MorphemeClass::FireLightHead.bit();

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MorphemeClassDef {
    pub bit_index: u32,
    pub description: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubBundleSpec {
    pub modifier_class: String,
    pub head_class: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FiberBundleSpec {
    pub bundle_id: String,
    pub description: String,
    pub sub_bundles: Vec<SubBundleSpec>,
    pub exclude_pairs: Vec<[String; 2]>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MorphemeOntologySchema {
    pub schema_version: String,
    pub title: String,
    pub description: String,
    pub classes: HashMap<String, MorphemeClassDef>,
    #[serde(default)]
    pub fiber_bundles: Vec<FiberBundleSpec>,
}

/// 統一漢語語素語意本體庫（Single Source of Truth Morpheme Ontology）
#[derive(Debug, Clone)]
pub struct MorphemeOntology {
    pub word_masks: HashMap<String, u64>,
    pub class_members: HashMap<String, Vec<String>>,
    pub fiber_bundles: Vec<FiberBundleSpec>,
    pub excluded_pairs: HashSet<(String, String)>,
}

static GLOBAL_ONTOLOGY: OnceLock<MorphemeOntology> = OnceLock::new();

impl MorphemeOntology {
    /// 取得全域單例語素本體庫（零 I/O 延遲，自動載入編譯期內嵌或外部 `morpheme_ontology.json`）
    pub fn global() -> &'static MorphemeOntology {
        GLOBAL_ONTOLOGY.get_or_init(Self::load_builtin)
    }

    pub fn load_builtin() -> Self {
        Self::from_json_str(BUILTIN_MORPHEME_ONTOLOGY_JSON)
            .expect("Failed to parse builtin morpheme_ontology.json")
    }

    pub fn load_from_path(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        Self::from_json_str(&content).ok()
    }

    pub fn from_json_str(json_str: &str) -> Result<Self, String> {
        let schema: MorphemeOntologySchema =
            serde_json::from_str(json_str).map_err(|e| format!("JSON parse error: {e}"))?;
        let mut word_masks: HashMap<String, u64> = HashMap::with_capacity(512);
        let mut class_members: HashMap<String, Vec<String>> =
            HashMap::with_capacity(schema.classes.len());

        for (class_name, def) in schema.classes {
            let bit = if let Some(cls) = MorphemeClass::from_name(&class_name) {
                cls.bit()
            } else if def.bit_index < 64 {
                1u64 << def.bit_index
            } else {
                0
            };
            for member in &def.members {
                let trimmed = member.trim();
                if !trimmed.is_empty() {
                    *word_masks.entry(trimmed.to_string()).or_insert(0) |= bit;
                }
            }
            class_members.insert(class_name, def.members);
        }

        let mut excluded_pairs = HashSet::new();
        for bundle in &schema.fiber_bundles {
            for pair in &bundle.exclude_pairs {
                excluded_pairs.insert((pair[0].clone(), pair[1].clone()));
            }
        }

        Ok(Self {
            word_masks,
            class_members,
            fiber_bundles: schema.fiber_bundles,
            excluded_pairs,
        })
    }

    #[inline]
    pub fn mask_of(&self, word: &str) -> u64 {
        self.word_masks.get(word).copied().unwrap_or(0)
    }

    #[inline]
    pub fn has_class(&self, word: &str, class: MorphemeClass) -> bool {
        (self.mask_of(word) & class.bit()) != 0
    }

    #[inline]
    pub fn has_any_class(&self, word: &str, mask: u64) -> bool {
        (self.mask_of(word) & mask) != 0
    }

    #[inline]
    pub fn is_excluded_pair(&self, left: &str, right: &str) -> bool {
        self.excluded_pairs
            .contains(&(left.to_string(), right.to_string()))
    }

    /// 判斷是否為發光體／火源正交纖維叢之合法原子或合成名詞：
    /// `([LampMod × 燈] ∪ [FireLightMod × {火, 光}] ∪ {燈, 火, 光}) \ E_excl`
    #[inline]
    pub fn is_factorized_luminous_noun(&self, word: &str) -> bool {
        if self.has_any_class(word, LUMINOUS_HEAD_MASK) {
            return true;
        }
        if let Some(stem) = word.strip_suffix('燈') {
            return self.has_class(stem, MorphemeClass::LampMod)
                && !self.is_excluded_pair(stem, "燈");
        }
        if let Some(stem) = word.strip_suffix('火') {
            return self.has_class(stem, MorphemeClass::FireLightMod)
                && !self.is_excluded_pair(stem, "火");
        }
        if let Some(stem) = word.strip_suffix('光') {
            return self.has_class(stem, MorphemeClass::FireLightMod)
                && !self.is_excluded_pair(stem, "光");
        }
        false
    }

    /// 判斷是否為氣象四正交子叢之合法中心語（雨／雪／風／霧／霜／露／雷／電／雹／太陽／陽光）
    #[inline]
    pub fn is_weather_head(&self, word: &str) -> bool {
        self.has_any_class(word, WEATHER_HEAD_MASK)
    }

    /// 驗證氣象修飾語素與氣象中心語素是否屬於同一個合法正交子叢（防止 *毛毛陽光、*濃風、*稀雨）
    #[inline]
    pub fn is_valid_weather_fiber_pair(&self, mod_word: &str, head_word: &str) -> bool {
        if self.is_excluded_pair(mod_word, head_word) {
            return false;
        }
        let m_mask = self.mask_of(mod_word);
        let h_mask = self.mask_of(head_word);
        ((m_mask & MorphemeClass::WeatherPrecipMod.bit()) != 0
            && (h_mask & MorphemeClass::WeatherPrecipHead.bit()) != 0)
            || ((m_mask & MorphemeClass::WeatherWindMod.bit()) != 0
                && (h_mask & MorphemeClass::WeatherWindHead.bit()) != 0)
            || ((m_mask & MorphemeClass::WeatherVaporMod.bit()) != 0
                && (h_mask & MorphemeClass::WeatherVaporHead.bit()) != 0)
            || ((m_mask & MorphemeClass::WeatherSevereMod.bit()) != 0
                && (h_mask & MorphemeClass::WeatherSevereHead.bit()) != 0)
    }

    /// 取得指定語素類別之全體成員清單（供 `ruhi-studio` 槽位同化與類推擴展使用）
    pub fn members_of(&self, class: MorphemeClass) -> &[String] {
        self.class_members
            .get(class.as_str())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_morpheme_ontology_loads_and_bitmask_queries_261101() {
        let ont = MorphemeOntology::global();
        assert!(ont.class_members.len() >= 44);
        assert!(ont.word_masks.len() >= 250);

        // 1. 驗證發光體與火源正交子叢 (LampMod × LampHead vs FireLightMod × FireLightHead)
        for w in ["燈", "路燈", "街燈", "車燈", "夜燈", "檯燈", "壁燈", "吊燈", "燭火", "星火", "燈火", "營火", "星光", "燭光"] {
            assert!(
                ont.is_factorized_luminous_noun(w),
                "Expected `{w}` to be recognized as a valid factorized luminous noun"
            );
        }
        for invalid in ["路火", "街火", "車火", "檯火", "燭燈", "星燈", "燈燈", "石頭"] {
            assert!(
                !ont.is_factorized_luminous_noun(invalid),
                "Expected `{invalid}` to be rejected by orthogonal fiber bundle exclusions"
            );
        }

        // 2. 驗證氣象四正交子叢配對
        assert!(ont.is_valid_weather_fiber_pair("毛毛", "雨"));
        assert!(ont.is_valid_weather_fiber_pair("鵝毛", "雪"));
        assert!(ont.is_valid_weather_fiber_pair("強", "風"));
        assert!(ont.is_valid_weather_fiber_pair("濃", "霧"));
        assert!(ont.is_valid_weather_fiber_pair("烈", "太陽"));
        assert!(!ont.is_valid_weather_fiber_pair("毛毛", "陽光"));
        assert!(!ont.is_valid_weather_fiber_pair("濃", "風"));
        assert!(!ont.is_valid_weather_fiber_pair("傾盆", "雪"));

        // 3. 驗證位移／視線詞根與強制雙態綁定中心語
        for v in ["流", "走", "奔", "飛", "駛", "望", "看", "朝", "邁", "通", "湧", "飄"] {
            assert!(ont.has_class(v, MorphemeClass::MotionGazeVerbStem));
        }
        assert!(ont.has_class("向", MorphemeClass::BoundHeadMorpheme));
        assert!(ont.has_class("連", MorphemeClass::ConcessiveFocusTrigger));
        assert!(ont.has_class("走過去", MorphemeClass::SerialMotionV1));
    }
}
