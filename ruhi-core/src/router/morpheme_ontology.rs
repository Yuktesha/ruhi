use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

const BUILTIN_MORPHEME_ONTOLOGY_JSON: &str =
    include_str!("../../../ruhi_lexicon/data/morpheme_ontology.json");

/// 零堆積配置（Zero-Heap `Copy`）之 256-bit 語素語意位元遮罩（支援 0..=255 正交語意類別）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MorphemeMask256(pub [u64; 4]);

impl MorphemeMask256 {
    pub const EMPTY: Self = Self([0; 4]);

    #[inline]
    pub const fn empty() -> Self {
        Self([0; 4])
    }

    #[inline]
    pub const fn from_bit(bit_index: u32) -> Self {
        if bit_index >= 256 {
            return Self::EMPTY;
        }
        let lane = (bit_index / 64) as usize;
        let offset = bit_index % 64;
        let mut lanes = [0u64; 4];
        lanes[lane] = 1u64 << offset;
        Self(lanes)
    }

    #[inline]
    pub const fn bitor(self, rhs: Self) -> Self {
        Self([
            self.0[0] | rhs.0[0],
            self.0[1] | rhs.0[1],
            self.0[2] | rhs.0[2],
            self.0[3] | rhs.0[3],
        ])
    }

    #[inline]
    pub const fn bitand(self, rhs: Self) -> Self {
        Self([
            self.0[0] & rhs.0[0],
            self.0[1] & rhs.0[1],
            self.0[2] & rhs.0[2],
            self.0[3] & rhs.0[3],
        ])
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        (self.0[0] | self.0[1] | self.0[2] | self.0[3]) == 0
    }

    #[inline]
    pub const fn intersects(self, rhs: Self) -> bool {
        !self.bitand(rhs).is_empty()
    }

    #[inline]
    pub const fn contains(self, sub: Self) -> bool {
        (self.0[0] & sub.0[0]) == sub.0[0]
            && (self.0[1] & sub.0[1]) == sub.0[1]
            && (self.0[2] & sub.0[2]) == sub.0[2]
            && (self.0[3] & sub.0[3]) == sub.0[3]
    }

    #[inline]
    pub const fn count_ones(self) -> u32 {
        self.0[0].count_ones()
            + self.0[1].count_ones()
            + self.0[2].count_ones()
            + self.0[3].count_ones()
    }
}

impl std::ops::BitOr for MorphemeMask256 {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self::bitor(self, rhs)
    }
}

impl std::ops::BitOrAssign for MorphemeMask256 {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        *self = Self::bitor(*self, rhs);
    }
}

impl std::ops::BitAnd for MorphemeMask256 {
    type Output = Self;
    #[inline]
    fn bitand(self, rhs: Self) -> Self::Output {
        Self::bitand(self, rhs)
    }
}

impl std::ops::BitAndAssign for MorphemeMask256 {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        *self = Self::bitand(*self, rhs);
    }
}

/// 漢語語素語意本體分類枚舉（對應 256-bit `MorphemeMask256` 位元索引 0..=65）
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
    PeriodicScheduleNoun = 45,
    AmericasGeoMod = 46,
    AmericasGeoHead = 47,
    ResourceStockNoun = 48,
    RestrictiveAdv = 49,
    CompetitionMetricNoun = 50,
    CompetitionDegreeAdv = 51,
    NewStartVerb = 52,
    NewObjectNoun = 53,
    WaitConditionNoun = 54,
    WaitResultVerb = 55,
    TemporalPostposition = 56,
    QuantityExprWord = 57,
    HierarchicalTreeMod = 58,
    QuantitativeCountMod = 59,
    UiHierarchyAnchor = 60,
    TreeHead = 61,
    UrgencyTopicAnchor = 62,
    BujiIdiomPrefix = 63,
    BujiObjectHead = 64,
    SentenceFinalParticle = 65,
    RareImpostorChar = 66,
    ModernLowWeightCompound2 = 67,
}

impl MorphemeClass {
    #[inline]
    pub const fn bit(self) -> MorphemeMask256 {
        MorphemeMask256::from_bit(self as u32)
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
            Self::PeriodicScheduleNoun => "PeriodicScheduleNoun",
            Self::AmericasGeoMod => "AmericasGeoMod",
            Self::AmericasGeoHead => "AmericasGeoHead",
            Self::ResourceStockNoun => "ResourceStockNoun",
            Self::RestrictiveAdv => "RestrictiveAdv",
            Self::CompetitionMetricNoun => "CompetitionMetricNoun",
            Self::CompetitionDegreeAdv => "CompetitionDegreeAdv",
            Self::NewStartVerb => "NewStartVerb",
            Self::NewObjectNoun => "NewObjectNoun",
            Self::WaitConditionNoun => "WaitConditionNoun",
            Self::WaitResultVerb => "WaitResultVerb",
            Self::TemporalPostposition => "TemporalPostposition",
            Self::QuantityExprWord => "QuantityExprWord",
            Self::HierarchicalTreeMod => "HierarchicalTreeMod",
            Self::QuantitativeCountMod => "QuantitativeCountMod",
            Self::UiHierarchyAnchor => "UiHierarchyAnchor",
            Self::TreeHead => "TreeHead",
            Self::UrgencyTopicAnchor => "UrgencyTopicAnchor",
            Self::BujiIdiomPrefix => "BujiIdiomPrefix",
            Self::BujiObjectHead => "BujiObjectHead",
            Self::SentenceFinalParticle => "SentenceFinalParticle",
            Self::RareImpostorChar => "RareImpostorChar",
            Self::ModernLowWeightCompound2 => "ModernLowWeightCompound2",
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
            "PeriodicScheduleNoun" => Some(Self::PeriodicScheduleNoun),
            "AmericasGeoMod" => Some(Self::AmericasGeoMod),
            "AmericasGeoHead" => Some(Self::AmericasGeoHead),
            "ResourceStockNoun" => Some(Self::ResourceStockNoun),
            "RestrictiveAdv" => Some(Self::RestrictiveAdv),
            "CompetitionMetricNoun" => Some(Self::CompetitionMetricNoun),
            "CompetitionDegreeAdv" => Some(Self::CompetitionDegreeAdv),
            "NewStartVerb" => Some(Self::NewStartVerb),
            "NewObjectNoun" => Some(Self::NewObjectNoun),
            "WaitConditionNoun" => Some(Self::WaitConditionNoun),
            "WaitResultVerb" => Some(Self::WaitResultVerb),
            "TemporalPostposition" => Some(Self::TemporalPostposition),
            "QuantityExprWord" => Some(Self::QuantityExprWord),
            "HierarchicalTreeMod" => Some(Self::HierarchicalTreeMod),
            "QuantitativeCountMod" => Some(Self::QuantitativeCountMod),
            "UiHierarchyAnchor" => Some(Self::UiHierarchyAnchor),
            "TreeHead" => Some(Self::TreeHead),
            "UrgencyTopicAnchor" => Some(Self::UrgencyTopicAnchor),
            "BujiIdiomPrefix" => Some(Self::BujiIdiomPrefix),
            "BujiObjectHead" => Some(Self::BujiObjectHead),
            "SentenceFinalParticle" => Some(Self::SentenceFinalParticle),
            "RareImpostorChar" => Some(Self::RareImpostorChar),
            "ModernLowWeightCompound2" => Some(Self::ModernLowWeightCompound2),
            _ => None,
        }
    }
}

pub const WEATHER_HEAD_MASK: MorphemeMask256 = MorphemeClass::WeatherPrecipHead
    .bit()
    .bitor(MorphemeClass::WeatherWindHead.bit())
    .bitor(MorphemeClass::WeatherVaporHead.bit())
    .bitor(MorphemeClass::WeatherSevereHead.bit());

pub const WEATHER_MOD_MASK: MorphemeMask256 = MorphemeClass::WeatherPrecipMod
    .bit()
    .bitor(MorphemeClass::WeatherWindMod.bit())
    .bitor(MorphemeClass::WeatherVaporMod.bit())
    .bitor(MorphemeClass::WeatherSevereMod.bit());

pub const LUMINOUS_MOD_MASK: MorphemeMask256 =
    MorphemeClass::LampMod.bit().bitor(MorphemeClass::FireLightMod.bit());

pub const LUMINOUS_HEAD_MASK: MorphemeMask256 =
    MorphemeClass::LampHead.bit().bitor(MorphemeClass::FireLightHead.bit());

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
    pub word_masks: HashMap<String, MorphemeMask256>,
    pub class_members: HashMap<String, Vec<String>>,
    pub fiber_bundles: Vec<FiberBundleSpec>,
    pub excluded_pairs: HashSet<(String, String)>,
    bmp_mask_idx: Vec<u16>,
    bmp_masks: Vec<MorphemeMask256>,
    rare_impostor_bmp: Vec<u64>,
    rare_impostor_has_extra: bool,
    multi_buckets: Vec<u16>,
    multi_entries: Vec<(u64, Box<str>, MorphemeMask256)>,
}

static GLOBAL_ONTOLOGY: OnceLock<MorphemeOntology> = OnceLock::new();

impl MorphemeOntology {
    #[inline(always)]
    fn single_bmp_cp(bytes: &[u8]) -> Option<usize> {
        match bytes.len() {
            1 => Some(bytes[0] as usize),
            2 if bytes[0] >= 0xC0 => {
                Some((((bytes[0] & 0x1F) as usize) << 6) | ((bytes[1] & 0x3F) as usize))
            }
            3 if (bytes[0] & 0xF0) == 0xE0 => Some(
                (((bytes[0] & 0x0F) as usize) << 12)
                    | (((bytes[1] & 0x3F) as usize) << 6)
                    | ((bytes[2] & 0x3F) as usize),
            ),
            _ => None,
        }
    }

    #[inline(always)]
    fn fast_hash_bytes(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^ (h >> 33)
    }

    /// 取得全域單例語素本體庫（零 I/O 延遲，自動載入編譯期內嵌或外部 `morpheme_ontology.json`）
    #[inline]
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
        let mut word_masks: HashMap<String, MorphemeMask256> = HashMap::with_capacity(512);
        let mut class_members: HashMap<String, Vec<String>> =
            HashMap::with_capacity(schema.classes.len());

        for (class_name, def) in schema.classes {
            let bit = if let Some(cls) = MorphemeClass::from_name(&class_name) {
                cls.bit()
            } else if def.bit_index < 256 {
                MorphemeMask256::from_bit(def.bit_index)
            } else {
                MorphemeMask256::EMPTY
            };
            for member in &def.members {
                let trimmed = member.trim();
                if !trimmed.is_empty() {
                    *word_masks.entry(trimmed.to_string()).or_insert(MorphemeMask256::EMPTY) |= bit;
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

        let mut bmp_mask_idx = vec![0u16; 65536];
        let mut bmp_masks = vec![MorphemeMask256::EMPTY];
        let mut rare_impostor_bmp = vec![0u64; 1024];
        let mut rare_impostor_has_extra = false;
        let rare_bit = MorphemeClass::RareImpostorChar.bit();
        let mut multi_entries: Vec<(u64, Box<str>, MorphemeMask256)> = Vec::new();

        for (word, &mask) in &word_masks {
            let bytes = word.as_bytes();
            let is_rare = mask.intersects(rare_bit);
            if let Some(cp) = Self::single_bmp_cp(bytes) {
                if is_rare {
                    rare_impostor_bmp[cp >> 6] |= 1u64 << (cp & 63);
                }
                if bmp_masks.len() < (u16::MAX as usize) {
                    let slot = bmp_masks.len() as u16;
                    bmp_masks.push(mask);
                    bmp_mask_idx[cp] = slot;
                } else {
                    bmp_mask_idx[cp] = u16::MAX;
                    multi_entries.push((
                        Self::fast_hash_bytes(bytes),
                        word.clone().into_boxed_str(),
                        mask,
                    ));
                    if is_rare {
                        rare_impostor_has_extra = true;
                    }
                }
            } else if !bytes.is_empty() {
                if is_rare {
                    rare_impostor_has_extra = true;
                }
                multi_entries.push((
                    Self::fast_hash_bytes(bytes),
                    word.clone().into_boxed_str(),
                    mask,
                ));
            }
        }

        let mut multi_buckets = Vec::new();
        if !multi_entries.is_empty() && multi_entries.len() < (u16::MAX as usize) {
            let cap = (multi_entries.len() * 4).max(256).next_power_of_two();
            multi_buckets = vec![0u16; cap];
            let cap_mask = cap - 1;
            for (idx, (h, _, _)) in multi_entries.iter().enumerate() {
                let mut slot = (*h as usize) & cap_mask;
                while multi_buckets[slot] != 0 {
                    slot = (slot + 1) & cap_mask;
                }
                multi_buckets[slot] = (idx + 1) as u16;
            }
        }

        Ok(Self {
            word_masks,
            class_members,
            fiber_bundles: schema.fiber_bundles,
            excluded_pairs,
            bmp_mask_idx,
            bmp_masks,
            rare_impostor_bmp,
            rare_impostor_has_extra,
            multi_buckets,
            multi_entries,
        })
    }

    #[inline]
    fn mask_of_multi(&self, bytes: &[u8]) -> MorphemeMask256 {
        if bytes.is_empty() {
            return MorphemeMask256::EMPTY;
        }
        if self.multi_buckets.is_empty() {
            return std::str::from_utf8(bytes)
                .ok()
                .and_then(|s| self.word_masks.get(s).copied())
                .unwrap_or(MorphemeMask256::EMPTY);
        }
        let h = Self::fast_hash_bytes(bytes);
        let mask = self.multi_buckets.len() - 1;
        let mut slot = (h as usize) & mask;
        loop {
            let b = self.multi_buckets[slot];
            if b == 0 {
                return MorphemeMask256::EMPTY;
            }
            let &(eh, ref ew, emask) = &self.multi_entries[(b - 1) as usize];
            if eh == h && ew.as_bytes() == bytes {
                return emask;
            }
            slot = (slot + 1) & mask;
        }
    }

    #[inline]
    pub fn mask_of(&self, word: &str) -> MorphemeMask256 {
        let bytes = word.as_bytes();
        if let Some(cp) = Self::single_bmp_cp(bytes) {
            let idx = self.bmp_mask_idx[cp];
            if idx != u16::MAX {
                return self.bmp_masks[idx as usize];
            }
        }
        self.mask_of_multi(bytes)
    }

    #[inline]
    pub fn is_rare_impostor_fast(&self, word: &str) -> bool {
        let bytes = word.as_bytes();
        if let Some(cp) = Self::single_bmp_cp(bytes) {
            return ((self.rare_impostor_bmp[cp >> 6] >> (cp & 63)) & 1) != 0;
        }
        self.rare_impostor_has_extra
            && self
                .mask_of_multi(bytes)
                .intersects(MorphemeClass::RareImpostorChar.bit())
    }

    #[inline]
    pub fn has_class(&self, word: &str, class: MorphemeClass) -> bool {
        self.mask_of(word).intersects(class.bit())
    }

    #[inline]
    pub fn has_any_class(&self, word: &str, mask: MorphemeMask256) -> bool {
        self.mask_of(word).intersects(mask)
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
        (m_mask.intersects(MorphemeClass::WeatherPrecipMod.bit())
            && h_mask.intersects(MorphemeClass::WeatherPrecipHead.bit()))
            || (m_mask.intersects(MorphemeClass::WeatherWindMod.bit())
                && h_mask.intersects(MorphemeClass::WeatherWindHead.bit()))
            || (m_mask.intersects(MorphemeClass::WeatherVaporMod.bit())
                && h_mask.intersects(MorphemeClass::WeatherVaporHead.bit()))
            || (m_mask.intersects(MorphemeClass::WeatherSevereMod.bit())
                && h_mask.intersects(MorphemeClass::WeatherSevereHead.bit()))
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

    #[test]
    fn test_morpheme_mask256_cross_lane_and_new_classes_261111() {
        // 1. 測試跨 lane 位元遮罩代數運算 (lane 0: bit 10, lane 1: bit 64, bit 65, lane 3: bit 255)
        let mask_empty = MorphemeMask256::EMPTY;
        assert!(mask_empty.is_empty());
        assert_eq!(mask_empty.count_ones(), 0);

        let m10 = MorphemeMask256::from_bit(10);
        let m64 = MorphemeMask256::from_bit(64);
        let m65 = MorphemeMask256::from_bit(65);
        let m255 = MorphemeMask256::from_bit(255);
        let m_oob = MorphemeMask256::from_bit(256);
        assert!(m_oob.is_empty());

        assert_eq!(m10.0[0], 1u64 << 10);
        assert_eq!(m64.0[1], 1u64 << 0);
        assert_eq!(m65.0[1], 1u64 << 1);
        assert_eq!(m255.0[3], 1u64 << 63);

        let combined = m10 | m64 | m65 | m255;
        assert_eq!(combined.count_ones(), 4);
        assert!(combined.intersects(m10));
        assert!(combined.intersects(m64));
        assert!(combined.intersects(m65));
        assert!(combined.intersects(m255));
        assert!(combined.contains(m64 | m65));
        assert!(!m10.intersects(m64));

        // 2. 測試新語意類別在本體庫中之精確解析
        let ont = MorphemeOntology::global();
        assert!(ont.has_class("語法", MorphemeClass::HierarchicalTreeMod));
        assert!(ont.has_class("二元", MorphemeClass::HierarchicalTreeMod));
        assert!(ont.has_class("計算", MorphemeClass::QuantitativeCountMod));
        assert!(ont.has_class("參數", MorphemeClass::QuantitativeCountMod));
        assert!(ont.has_class("側欄", MorphemeClass::UiHierarchyAnchor));
        assert!(ont.has_class("介面", MorphemeClass::UiHierarchyAnchor));
        assert!(ont.has_class("樹", MorphemeClass::TreeHead));
        assert!(ont.has_class("這", MorphemeClass::UrgencyTopicAnchor));
        assert!(ont.has_class("暫時", MorphemeClass::UrgencyTopicAnchor));
        assert!(ont.has_class("來", MorphemeClass::BujiIdiomPrefix));
        assert!(ont.has_class("措手", MorphemeClass::BujiIdiomPrefix));
        assert!(ont.has_class("格", MorphemeClass::BujiObjectHead));
        assert!(ont.has_class("預期", MorphemeClass::BujiObjectHead));
        assert!(ont.has_class("嗎", MorphemeClass::SentenceFinalParticle));
        assert!(ont.has_class("吧", MorphemeClass::SentenceFinalParticle));
        assert!(ont.has_class("洅", MorphemeClass::RareImpostorChar));
        assert!(ont.has_class("瘌", MorphemeClass::RareImpostorChar));
        assert!(ont.has_class("諺文", MorphemeClass::ModernLowWeightCompound2));
        assert!(ont.has_class("法性", MorphemeClass::ModernLowWeightCompound2));
        for (word, &expected_mask) in &ont.word_masks {
            assert_eq!(ont.mask_of(word).0, expected_mask.0, "mask parity mismatch for {}", word);
            assert_eq!(
                ont.is_rare_impostor_fast(word),
                expected_mask.intersects(MorphemeClass::RareImpostorChar.bit()),
                "rare impostor fast parity mismatch for {}",
                word
            );
        }
        assert!(ont.is_rare_impostor_fast("𣇫"));
        assert!(ont.is_rare_impostor_fast("不憤"));
        assert!(!ont.is_rare_impostor_fast(""));
        assert!(!ont.is_rare_impostor_fast("測試詞"));
        assert!(!ont.is_rare_impostor_fast("AI"));
        assert!(!ont.is_rare_impostor_fast("的"));
    }
}
