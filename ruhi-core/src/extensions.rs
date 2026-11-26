//! Declarative Four-Layer Extension Bundle Loader (`manifest.toml`)
//!
//! Implements Track 2D (P2-4) for RuhI's declarative multi-language and keyboard
//! extension architecture defined in `docs/EXTENSION_ARCHITECTURE_SPEC.md`:
//! - Layer 1: `keymap.toml` (Keystroke-to-unit transducer / FSM rules)
//! - Layer 2: `lexicon.tsv` & `morphology.toml` (Lexicon & inflection pack)
//! - Layer 3: `phonetics.toml` (Declarative phonological approximation profile)
//! - Layer 4: `patterns.jsonl` (Contextual syntax & autocorrect patterns)

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

/// Unified Finite State Machine archetypes supported by RuhI extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FsmArchetype {
    /// Syllabic combinator FSM (e.g., Bopomofo, Pinyin, Hangul Jamo, Romaji-to-Kana).
    SyllabicCombinator,
    /// Shape-based code-table FSM (e.g., Cangjie, Array30, Four-Corner, Wubi).
    CodeTableShape,
    /// Tokenizing lexeme FSM for alphabetic/Latin/Cyrillic languages with identifier passthrough.
    TokenizingLexeme,
}

impl FsmArchetype {
    /// Parses a string representation into an `FsmArchetype`.
    ///
    /// Accepts canonical `snake_case`, `kebab-case`, and `PascalCase` forms.
    pub fn from_str(s: &str) -> Result<Self, ExtensionError> {
        let normalized = s.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "syllabic_combinator" | "syllabiccombinator" => Ok(Self::SyllabicCombinator),
            "code_table_shape" | "codetableshape" => Ok(Self::CodeTableShape),
            "tokenizing_lexeme" | "tokenizinglexeme" => Ok(Self::TokenizingLexeme),
            _ => Err(ExtensionError::InvalidArchetype {
                value: s.trim().to_string(),
            }),
        }
    }

    /// Returns the canonical `snake_case` identifier for this archetype.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SyllabicCombinator => "syllabic_combinator",
            Self::CodeTableShape => "code_table_shape",
            Self::TokenizingLexeme => "tokenizing_lexeme",
        }
    }
}

impl FromStr for FsmArchetype {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        FsmArchetype::from_str(s)
    }
}

impl fmt::Display for FsmArchetype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Errors produced while parsing, validating, or loading an extension bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionError {
    Io {
        path: PathBuf,
        message: String,
    },
    TomlParse {
        line: usize,
        message: String,
    },
    MissingRequiredField {
        field: String,
    },
    InvalidFieldValue {
        field: String,
        value: String,
        reason: String,
    },
    InvalidArchetype {
        value: String,
    },
    PathTraversal {
        field: String,
        declared_path: String,
    },
    MissingAsset {
        field: String,
        path: PathBuf,
    },
}

impl fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(f, "I/O error at '{}': {}", path.display(), message)
            }
            Self::TomlParse { line, message } => {
                write!(f, "TOML parse error at line {}: {}", line, message)
            }
            Self::MissingRequiredField { field } => {
                write!(f, "missing required manifest field '{}'", field)
            }
            Self::InvalidFieldValue {
                field,
                value,
                reason,
            } => write!(
                f,
                "invalid value '{}' for field '{}': {}",
                value, field, reason
            ),
            Self::InvalidArchetype { value } => {
                write!(
                    f,
                    "invalid FSM archetype '{}'; expected one of: syllabic_combinator, code_table_shape, tokenizing_lexeme",
                    value
                )
            }
            Self::PathTraversal {
                field,
                declared_path,
            } => write!(
                f,
                "unsafe path traversal rejected for '{}' ('{}')",
                field, declared_path
            ),
            Self::MissingAsset { field, path } => {
                write!(
                    f,
                    "declared asset '{}' does not exist at '{}'",
                    field,
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ExtensionError {}

/// Severity level for non-fatal or structured bundle diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExtensionValidationSeverity {
    Warning,
    Error,
}

/// Structured validation finding associated with an extension bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionValidationError {
    pub severity: ExtensionValidationSeverity,
    pub field: String,
    pub message: String,
    pub path: Option<PathBuf>,
}

/// Aggregated diagnostics collected during bundle validation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionDiagnostics {
    pub warnings: Vec<ExtensionValidationError>,
    pub errors: Vec<ExtensionValidationError>,
    pub notes: Vec<String>,
}

impl ExtensionDiagnostics {
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty() && self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Typed scalar/array value parsed from the zero-dependency TOML subset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TomlValue {
    String(String),
    Integer(i64),
    Boolean(bool),
    Array(Vec<String>),
}

impl TomlValue {
    fn type_name(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Integer(_) => "integer",
            Self::Boolean(_) => "boolean",
            Self::Array(_) => "array",
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    fn as_string_array(&self) -> Option<&[String]> {
        match self {
            Self::Array(arr) => Some(arr.as_slice()),
            _ => None,
        }
    }
}

/// Optional UI presentation hints declared under `[ui]` in `manifest.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiHints {
    pub indicator: Option<String>,
    pub display_name: Option<String>,
    pub capslock_switch: Option<String>,
    pub extra: BTreeMap<String, TomlValue>,
}

/// Optional FSM configuration hints declared under `[fsm]` in `manifest.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsmHints {
    pub max_buffer_len: Option<usize>,
    pub tab_prefix_completion: Option<bool>,
    pub passthrough_identifiers: Option<bool>,
    pub extra: BTreeMap<String, TomlValue>,
}

/// Parsed representation of `manifest.toml` for a 4-layer declarative RuhI extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// BCP-47 language tag (populated from `language_code` or `bcp47`).
    pub language_code: String,
    pub archetype: FsmArchetype,
    pub author: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub min_core_version: Option<String>,
    pub tags: Vec<String>,
    /// Layer 1: Keystroke-to-unit transducer (`keymap.toml`).
    pub keymap: Option<String>,
    /// Layer 2: Normalized lexicon TSV (`lexicon.tsv`).
    pub lexicon: Option<String>,
    /// Layer 2: Inflection & contraction rules (`morphology.toml`).
    pub morphology: Option<String>,
    /// Layer 3: Declarative phonological approximation profile (`phonetics.toml`).
    pub phonetics: Option<String>,
    /// Layer 4: Contextual syntax patterns & autocorrect whitelist (`patterns.jsonl`).
    pub patterns: Option<String>,
    pub ui: UiHints,
    pub fsm: FsmHints,
}

impl ExtensionManifest {
    /// Returns the BCP-47 language code for this extension.
    pub fn bcp47(&self) -> &str {
        &self.language_code
    }

    /// Returns all five four-layer asset declarations as `(field_name, Option<&str>)`.
    pub fn declared_assets(&self) -> [(&'static str, Option<&str>); 5] {
        [
            ("keymap", self.keymap.as_deref()),
            ("lexicon", self.lexicon.as_deref()),
            ("morphology", self.morphology.as_deref()),
            ("phonetics", self.phonetics.as_deref()),
            ("patterns", self.patterns.as_deref()),
        ]
    }
}

/// Resolved filesystem paths inside `bundle_dir` for declared assets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedExtensionAssets {
    pub keymap: Option<PathBuf>,
    pub lexicon: Option<PathBuf>,
    pub morphology: Option<PathBuf>,
    pub phonetics: Option<PathBuf>,
    pub patterns: Option<PathBuf>,
}

/// Loaded and validated extension bundle ready for runtime registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedExtensionBundle {
    pub manifest: ExtensionManifest,
    pub bundle_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub assets: ResolvedExtensionAssets,
    pub diagnostics: ExtensionDiagnostics,
}

#[derive(Debug, Clone)]
struct ParsedEntry {
    section: String,
    key: String,
    value: TomlValue,
    line: usize,
}

/// Parses a `manifest.toml` source string using a zero-dependency TOML subset parser.
///
/// Supports `[sections]`, `#` comments (full-line and inline), double-quoted and
/// single-quoted strings, single-line and multi-line string arrays `["a", "b"]`,
/// booleans, and integers, returning line-numbered errors on malformed syntax.
pub fn parse_manifest_toml(src: &str) -> Result<ExtensionManifest, ExtensionError> {
    let entries = parse_toml_entries(src)?;

    let mut id: Option<String> = None;
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut language_code: Option<String> = None;
    let mut bcp47: Option<String> = None;
    let mut archetype_raw: Option<String> = None;
    let mut author: Option<String> = None;
    let mut description: Option<String> = None;
    let mut license: Option<String> = None;
    let mut min_core_version: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();

    let mut keymap: Option<String> = None;
    let mut lexicon: Option<String> = None;
    let mut morphology: Option<String> = None;
    let mut phonetics: Option<String> = None;
    let mut patterns: Option<String> = None;

    let mut ui = UiHints::default();
    let mut fsm = FsmHints::default();

    for entry in entries {
        let sec = entry.section.as_str();
        let key = entry.key.as_str();

        match sec {
            "" | "extension" | "package" | "manifest" => match key {
                "id" => id = Some(expect_non_empty_string("id", &entry.value)?),
                "name" => name = Some(expect_non_empty_string("name", &entry.value)?),
                "version" => version = Some(expect_non_empty_string("version", &entry.value)?),
                "language_code" => {
                    language_code = Some(expect_non_empty_string("language_code", &entry.value)?)
                }
                "bcp47" => bcp47 = Some(expect_non_empty_string("bcp47", &entry.value)?),
                "archetype" => {
                    archetype_raw = Some(expect_non_empty_string("archetype", &entry.value)?)
                }
                "author" => author = Some(expect_string("author", &entry.value)?),
                "description" => description = Some(expect_string("description", &entry.value)?),
                "license" => license = Some(expect_string("license", &entry.value)?),
                "min_core_version" => {
                    min_core_version = Some(expect_string("min_core_version", &entry.value)?)
                }
                "tags" => tags = expect_string_array("tags", &entry.value)?,
                "keymap" => set_layer_field(&mut keymap, "keymap", &entry.value, entry.line)?,
                "lexicon" => set_layer_field(&mut lexicon, "lexicon", &entry.value, entry.line)?,
                "morphology" => {
                    set_layer_field(&mut morphology, "morphology", &entry.value, entry.line)?
                }
                "phonetics" => {
                    set_layer_field(&mut phonetics, "phonetics", &entry.value, entry.line)?
                }
                "patterns" => set_layer_field(&mut patterns, "patterns", &entry.value, entry.line)?,
                _ => {
                    return Err(ExtensionError::TomlParse {
                        line: entry.line,
                        message: format!("unknown manifest key '{}' in section [{}]", key, sec),
                    });
                }
            },
            "layers" | "files" => match key {
                "keymap" => set_layer_field(&mut keymap, "keymap", &entry.value, entry.line)?,
                "lexicon" => set_layer_field(&mut lexicon, "lexicon", &entry.value, entry.line)?,
                "morphology" => {
                    set_layer_field(&mut morphology, "morphology", &entry.value, entry.line)?
                }
                "phonetics" => {
                    set_layer_field(&mut phonetics, "phonetics", &entry.value, entry.line)?
                }
                "patterns" => set_layer_field(&mut patterns, "patterns", &entry.value, entry.line)?,
                _ => {
                    return Err(ExtensionError::TomlParse {
                        line: entry.line,
                        message: format!("unknown layer key '{}' in [{}]", key, sec),
                    });
                }
            },
            "ui" => match key {
                "indicator" => ui.indicator = Some(expect_string("ui.indicator", &entry.value)?),
                "display_name" => {
                    ui.display_name = Some(expect_string("ui.display_name", &entry.value)?)
                }
                "capslock_switch" => {
                    ui.capslock_switch = Some(expect_string("ui.capslock_switch", &entry.value)?)
                }
                _ => {
                    ui.extra.insert(key.to_string(), entry.value);
                }
            },
            "fsm" => match key {
                "max_buffer_len" => {
                    let raw = expect_integer("fsm.max_buffer_len", &entry.value)?;
                    if raw <= 0 {
                        return Err(ExtensionError::InvalidFieldValue {
                            field: "fsm.max_buffer_len".to_string(),
                            value: raw.to_string(),
                            reason: "must be a positive integer".to_string(),
                        });
                    }
                    fsm.max_buffer_len = Some(raw as usize);
                }
                "tab_prefix_completion" => {
                    fsm.tab_prefix_completion =
                        Some(expect_bool("fsm.tab_prefix_completion", &entry.value)?);
                }
                "passthrough_identifiers" => {
                    fsm.passthrough_identifiers =
                        Some(expect_bool("fsm.passthrough_identifiers", &entry.value)?);
                }
                _ => {
                    fsm.extra.insert(key.to_string(), entry.value);
                }
            },
            other => {
                return Err(ExtensionError::TomlParse {
                    line: entry.line,
                    message: format!("unsupported section '[{}]'", other),
                });
            }
        }
    }

    let id = id.ok_or_else(|| ExtensionError::MissingRequiredField {
        field: "id".to_string(),
    })?;
    validate_extension_id(&id)?;

    let name = name.ok_or_else(|| ExtensionError::MissingRequiredField {
        field: "name".to_string(),
    })?;
    let version = version.ok_or_else(|| ExtensionError::MissingRequiredField {
        field: "version".to_string(),
    })?;

    let resolved_lang = match (language_code, bcp47) {
        (Some(lc), Some(b)) => {
            if !lc.eq_ignore_ascii_case(&b) {
                return Err(ExtensionError::InvalidFieldValue {
                    field: "bcp47".to_string(),
                    value: b,
                    reason: format!("conflicts with language_code '{}'", lc),
                });
            }
            lc
        }
        (Some(lc), None) => lc,
        (None, Some(b)) => b,
        (None, None) => {
            return Err(ExtensionError::MissingRequiredField {
                field: "language_code".to_string(),
            });
        }
    };

    let archetype_str = archetype_raw.ok_or_else(|| ExtensionError::MissingRequiredField {
        field: "archetype".to_string(),
    })?;
    let archetype = FsmArchetype::from_str(&archetype_str)?;

    Ok(ExtensionManifest {
        id,
        name,
        version,
        language_code: resolved_lang,
        archetype,
        author,
        description,
        license,
        min_core_version,
        tags,
        keymap,
        lexicon,
        morphology,
        phonetics,
        patterns,
        ui,
        fsm,
    })
}

/// Loads and validates a declarative extension bundle from `bundle_dir`.
///
/// Validates `manifest.toml`, rejects any path traversal (`..`, rooted paths,
/// drive prefixes, or symlink escapes outside `bundle_dir`), verifies all declared
/// asset files exist as regular files, and populates structured diagnostics.
pub fn load_extension_bundle(bundle_dir: &Path) -> Result<LoadedExtensionBundle, ExtensionError> {
    if !bundle_dir.exists() {
        return Err(ExtensionError::Io {
            path: bundle_dir.to_path_buf(),
            message: "bundle directory does not exist".to_string(),
        });
    }
    if !bundle_dir.is_dir() {
        return Err(ExtensionError::Io {
            path: bundle_dir.to_path_buf(),
            message: "bundle path is not a directory".to_string(),
        });
    }

    let manifest_path = bundle_dir.join("manifest.toml");
    let raw_manifest = fs::read_to_string(&manifest_path).map_err(|e| ExtensionError::Io {
        path: manifest_path.clone(),
        message: e.to_string(),
    })?;

    let manifest = parse_manifest_toml(&raw_manifest)?;
    let canonical_bundle = bundle_dir.canonicalize().ok();

    let mut assets = ResolvedExtensionAssets::default();
    let mut diagnostics = ExtensionDiagnostics::default();

    let resolve_layer = |field: &str,
                         declared: Option<&str>|
     -> Result<Option<PathBuf>, ExtensionError> {
        let Some(rel_str) = declared else {
            return Ok(None);
        };
        let resolved =
            resolve_safe_bundle_path(bundle_dir, canonical_bundle.as_deref(), field, rel_str)?;
        if !resolved.exists() || !resolved.is_file() {
            return Err(ExtensionError::MissingAsset {
                field: field.to_string(),
                path: resolved,
            });
        }
        Ok(Some(resolved))
    };

    assets.keymap = resolve_layer("keymap", manifest.keymap.as_deref())?;
    assets.lexicon = resolve_layer("lexicon", manifest.lexicon.as_deref())?;
    assets.morphology = resolve_layer("morphology", manifest.morphology.as_deref())?;
    assets.phonetics = resolve_layer("phonetics", manifest.phonetics.as_deref())?;
    assets.patterns = resolve_layer("patterns", manifest.patterns.as_deref())?;

    if assets.keymap.is_none() {
        diagnostics.warnings.push(ExtensionValidationError {
            severity: ExtensionValidationSeverity::Warning,
            field: "keymap".to_string(),
            message: "Layer 1 'keymap' is not declared in manifest.toml".to_string(),
            path: None,
        });
    }
    if assets.lexicon.is_none()
        && assets.morphology.is_none()
        && assets.phonetics.is_none()
        && assets.patterns.is_none()
    {
        diagnostics.warnings.push(ExtensionValidationError {
            severity: ExtensionValidationSeverity::Warning,
            field: "layers".to_string(),
            message: "no Layer 2–4 linguistic data assets are declared".to_string(),
            path: None,
        });
    }

    Ok(LoadedExtensionBundle {
        manifest,
        bundle_dir: bundle_dir.to_path_buf(),
        manifest_path,
        assets,
        diagnostics,
    })
}

/// Discovers and loads extension bundles across one or more `extensions/` root directories.
///
/// Roots are scanned in slice order; missing or non-directory roots are skipped without error.
/// Within each root, immediate subdirectories (excluding `.`-prefixed hidden folders and
/// regular files such as `README.md`) are visited in deterministic lexicographical order by
/// directory name.
pub fn discover_extensions(
    search_roots: &[&Path],
) -> Vec<Result<LoadedExtensionBundle, (PathBuf, ExtensionError)>> {
    let mut results = Vec::new();

    for root in search_roots {
        if !root.exists() || !root.is_dir() {
            continue;
        }

        let read_dir = match fs::read_dir(root) {
            Ok(rd) => rd,
            Err(err) => {
                results.push(Err((
                    root.to_path_buf(),
                    ExtensionError::Io {
                        path: root.to_path_buf(),
                        message: err.to_string(),
                    },
                )));
                continue;
            }
        };

        let mut subdirs: Vec<(String, PathBuf)> = Vec::new();
        for entry_res in read_dir {
            match entry_res {
                Ok(entry) => {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let dir_name = entry.file_name().to_string_lossy().into_owned();
                    if dir_name.starts_with('.') {
                        continue;
                    }
                    subdirs.push((dir_name, path));
                }
                Err(err) => {
                    results.push(Err((
                        root.to_path_buf(),
                        ExtensionError::Io {
                            path: root.to_path_buf(),
                            message: err.to_string(),
                        },
                    )));
                }
            }
        }

        subdirs.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        for (_name, subdir) in subdirs {
            match load_extension_bundle(&subdir) {
                Ok(bundle) => results.push(Ok(bundle)),
                Err(err) => results.push(Err((subdir, err))),
            }
        }
    }

    results
}

fn validate_extension_id(id: &str) -> Result<(), ExtensionError> {
    if id.contains("..")
        || id.contains('/')
        || id.contains('\\')
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(ExtensionError::InvalidFieldValue {
            field: "id".to_string(),
            value: id.to_string(),
            reason: "must contain only ASCII alphanumerics, '-', '_', or '.' without '..'"
                .to_string(),
        });
    }
    Ok(())
}

fn resolve_safe_bundle_path(
    bundle_dir: &Path,
    canonical_bundle: Option<&Path>,
    field: &str,
    declared_rel: &str,
) -> Result<PathBuf, ExtensionError> {
    let trimmed = declared_rel.trim();
    if trimmed.is_empty() {
        return Err(ExtensionError::InvalidFieldValue {
            field: field.to_string(),
            value: declared_rel.to_string(),
            reason: "asset path must not be empty".to_string(),
        });
    }

    // Reject Windows-style drive letters or leading separators regardless of host OS.
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return Err(ExtensionError::PathTraversal {
            field: field.to_string(),
            declared_path: declared_rel.to_string(),
        });
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(ExtensionError::PathTraversal {
            field: field.to_string(),
            declared_path: declared_rel.to_string(),
        });
    }

    let normalized_for_split = trimmed.replace('\\', "/");
    if normalized_for_split.split('/').any(|seg| seg == "..") {
        return Err(ExtensionError::PathTraversal {
            field: field.to_string(),
            declared_path: declared_rel.to_string(),
        });
    }

    let rel_path = Path::new(trimmed);
    for comp in rel_path.components() {
        match comp {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ExtensionError::PathTraversal {
                    field: field.to_string(),
                    declared_path: declared_rel.to_string(),
                });
            }
        }
    }

    let joined = bundle_dir.join(rel_path);
    if let (Some(base_canon), Ok(target_canon)) = (canonical_bundle, joined.canonicalize()) {
        if !target_canon.starts_with(base_canon) {
            return Err(ExtensionError::PathTraversal {
                field: field.to_string(),
                declared_path: declared_rel.to_string(),
            });
        }
    }

    Ok(joined)
}

fn set_layer_field(
    slot: &mut Option<String>,
    field: &str,
    value: &TomlValue,
    line: usize,
) -> Result<(), ExtensionError> {
    if slot.is_some() {
        return Err(ExtensionError::TomlParse {
            line,
            message: format!("duplicate layer declaration '{}'", field),
        });
    }
    *slot = Some(expect_non_empty_string(field, value)?);
    Ok(())
}

fn expect_string(field: &str, value: &TomlValue) -> Result<String, ExtensionError> {
    value
        .as_str()
        .map(ToString::to_string)
        .ok_or_else(|| ExtensionError::InvalidFieldValue {
            field: field.to_string(),
            value: value.type_name().to_string(),
            reason: "expected a string".to_string(),
        })
}

fn expect_non_empty_string(field: &str, value: &TomlValue) -> Result<String, ExtensionError> {
    let s = expect_string(field, value)?;
    if s.trim().is_empty() {
        return Err(ExtensionError::InvalidFieldValue {
            field: field.to_string(),
            value: s,
            reason: "must not be empty".to_string(),
        });
    }
    Ok(s)
}

fn expect_bool(field: &str, value: &TomlValue) -> Result<bool, ExtensionError> {
    value.as_bool().ok_or_else(|| ExtensionError::InvalidFieldValue {
        field: field.to_string(),
        value: value.type_name().to_string(),
        reason: "expected a boolean (true/false)".to_string(),
    })
}

fn expect_integer(field: &str, value: &TomlValue) -> Result<i64, ExtensionError> {
    value.as_i64().ok_or_else(|| ExtensionError::InvalidFieldValue {
        field: field.to_string(),
        value: value.type_name().to_string(),
        reason: "expected an integer".to_string(),
    })
}

fn expect_string_array(field: &str, value: &TomlValue) -> Result<Vec<String>, ExtensionError> {
    value
        .as_string_array()
        .map(|s| s.to_vec())
        .ok_or_else(|| ExtensionError::InvalidFieldValue {
            field: field.to_string(),
            value: value.type_name().to_string(),
            reason: "expected an array of strings".to_string(),
        })
}

// ---------------------------------------------------------------------------
// Zero-dependency TOML subset parser
// ---------------------------------------------------------------------------

fn parse_toml_entries(src: &str) -> Result<Vec<ParsedEntry>, ExtensionError> {
    let cleaned = src.strip_prefix('\u{FEFF}').unwrap_or(src);
    let lines: Vec<&str> = cleaned.lines().collect();

    let mut entries = Vec::new();
    let mut seen_keys: BTreeSet<(String, String)> = BTreeSet::new();
    let mut current_section = String::new();
    let mut idx = 0usize;

    while idx < lines.len() {
        let line_no = idx + 1;
        let line_no_comment = strip_inline_comment(lines[idx]);
        let trimmed = line_no_comment.trim();

        if trimmed.is_empty() {
            idx += 1;
            continue;
        }

        if trimmed.starts_with('[') {
            if trimmed.starts_with("[[") || !trimmed.ends_with(']') {
                return Err(ExtensionError::TomlParse {
                    line: line_no,
                    message: format!("malformed section header '{}'", trimmed),
                });
            }
            let sec_name = trimmed[1..trimmed.len() - 1].trim();
            if sec_name.is_empty()
                || !sec_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
            {
                return Err(ExtensionError::TomlParse {
                    line: line_no,
                    message: format!("invalid section name '{}'", sec_name),
                });
            }
            current_section = sec_name.to_string();
            idx += 1;
            continue;
        }

        let Some(eq_pos) = find_unquoted_equals(lines[idx]) else {
            return Err(ExtensionError::TomlParse {
                line: line_no,
                message: format!("expected 'key = value' assignment, got '{}'", trimmed),
            });
        };

        let key = lines[idx][..eq_pos].trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ExtensionError::TomlParse {
                line: line_no,
                message: format!("invalid key name '{}'", key),
            });
        }

        if !seen_keys.insert((current_section.clone(), key.to_string())) {
            return Err(ExtensionError::TomlParse {
                line: line_no,
                message: format!("duplicate key '{}' in section [{}]", key, current_section),
            });
        }

        let after_eq = strip_inline_comment(&lines[idx][eq_pos + 1..]).trim();
        let value = if after_eq.starts_with('[') && !array_is_balanced(after_eq) {
            let mut combined = String::from(after_eq);
            let start_line = line_no;
            let mut closed = false;
            while idx + 1 < lines.len() {
                idx += 1;
                let next_part = strip_inline_comment(lines[idx]).trim();
                if next_part.is_empty() {
                    continue;
                }
                combined.push(' ');
                combined.push_str(next_part);
                if array_is_balanced(&combined) {
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err(ExtensionError::TomlParse {
                    line: start_line,
                    message: "unterminated array literal".to_string(),
                });
            }
            parse_toml_value(&combined, start_line)?
        } else {
            parse_toml_value(after_eq, line_no)?
        };

        entries.push(ParsedEntry {
            section: current_section.clone(),
            key: key.to_string(),
            value,
            line: line_no,
        });

        idx += 1;
    }

    Ok(entries)
}

fn find_unquoted_equals(line: &str) -> Option<usize> {
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    for (i, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_double {
            if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_double = false;
            }
        } else if in_single {
            if ch == '\'' {
                in_single = false;
            }
        } else {
            match ch {
                '#' => return None,
                '"' => in_double = true,
                '\'' => in_single = true,
                '=' => return Some(i),
                _ => {}
            }
        }
    }
    None
}

fn strip_inline_comment(s: &str) -> &str {
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    for (i, ch) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_double {
            if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_double = false;
            }
        } else if in_single {
            if ch == '\'' {
                in_single = false;
            }
        } else {
            match ch {
                '"' => in_double = true,
                '\'' => in_single = true,
                '#' => return &s[..i],
                _ => {}
            }
        }
    }
    s
}

fn array_is_balanced(s: &str) -> bool {
    let mut depth = 0i32;
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    for ch in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_double {
            if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_double = false;
            }
        } else if in_single {
            if ch == '\'' {
                in_single = false;
            }
        } else {
            match ch {
                '"' => in_double = true,
                '\'' => in_single = true,
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
        }
    }
    depth == 0
}

fn parse_toml_value(raw: &str, line: usize) -> Result<TomlValue, ExtensionError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(ExtensionError::TomlParse {
            line,
            message: "missing value after '='".to_string(),
        });
    }

    if s.starts_with('"') {
        let (parsed, rest) = parse_double_quoted_string(s, line)?;
        if !rest.trim().is_empty() {
            return Err(ExtensionError::TomlParse {
                line,
                message: format!("unexpected trailing characters '{}' after string", rest.trim()),
            });
        }
        return Ok(TomlValue::String(parsed));
    }

    if s.starts_with('\'') {
        let (parsed, rest) = parse_single_quoted_string(s, line)?;
        if !rest.trim().is_empty() {
            return Err(ExtensionError::TomlParse {
                line,
                message: format!("unexpected trailing characters '{}' after string", rest.trim()),
            });
        }
        return Ok(TomlValue::String(parsed));
    }

    if s.starts_with('[') {
        let arr = parse_string_array(s, line)?;
        return Ok(TomlValue::Array(arr));
    }

    if s == "true" {
        return Ok(TomlValue::Boolean(true));
    }
    if s == "false" {
        return Ok(TomlValue::Boolean(false));
    }

    if let Some(int_val) = parse_toml_integer(s) {
        return Ok(TomlValue::Integer(int_val));
    }

    Err(ExtensionError::TomlParse {
        line,
        message: format!("invalid or unsupported TOML value '{}'", s),
    })
}

fn parse_double_quoted_string(s: &str, line: usize) -> Result<(String, &str), ExtensionError> {
    let mut chars = s.char_indices();
    match chars.next() {
        Some((_, '"')) => {}
        _ => {
            return Err(ExtensionError::TomlParse {
                line,
                message: "expected opening '\"'".to_string(),
            })
        }
    }

    let mut out = String::new();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            '"' => return Ok((out, &s[idx + 1..])),
            '\\' => {
                let Some((_, esc)) = chars.next() else {
                    return Err(ExtensionError::TomlParse {
                        line,
                        message: "unterminated escape sequence in string".to_string(),
                    });
                };
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'u' => {
                        let mut hex = String::with_capacity(4);
                        for _ in 0..4 {
                            let Some((_, h)) = chars.next() else {
                                return Err(ExtensionError::TomlParse {
                                    line,
                                    message: "incomplete \\uXXXX unicode escape".to_string(),
                                });
                            };
                            hex.push(h);
                        }
                        let code =
                            u32::from_str_radix(&hex, 16).map_err(|_| ExtensionError::TomlParse {
                                line,
                                message: format!("invalid hex in \\u{} escape", hex),
                            })?;
                        let decoded =
                            char::from_u32(code).ok_or_else(|| ExtensionError::TomlParse {
                                line,
                                message: format!("invalid unicode scalar U+{:04X}", code),
                            })?;
                        out.push(decoded);
                    }
                    other => {
                        return Err(ExtensionError::TomlParse {
                            line,
                            message: format!("unsupported escape sequence '\\{}'", other),
                        });
                    }
                }
            }
            other => out.push(other),
        }
    }

    Err(ExtensionError::TomlParse {
        line,
        message: "unterminated double-quoted string".to_string(),
    })
}

fn parse_single_quoted_string(s: &str, line: usize) -> Result<(String, &str), ExtensionError> {
    let rest_after_open = s.strip_prefix('\'').ok_or_else(|| ExtensionError::TomlParse {
        line,
        message: "expected opening '''".to_string(),
    })?;
    if let Some(end_idx) = rest_after_open.find('\'') {
        let content = &rest_after_open[..end_idx];
        let trailing = &rest_after_open[end_idx + 1..];
        Ok((content.to_string(), trailing))
    } else {
        Err(ExtensionError::TomlParse {
            line,
            message: "unterminated single-quoted string".to_string(),
        })
    }
}

fn parse_string_array(s: &str, line: usize) -> Result<Vec<String>, ExtensionError> {
    if !s.starts_with('[') || !s.ends_with(']') {
        return Err(ExtensionError::TomlParse {
            line,
            message: "unterminated array literal".to_string(),
        });
    }

    let mut inner = s[1..s.len() - 1].trim();
    let mut items = Vec::new();

    while !inner.is_empty() {
        let (item, rest) = if inner.starts_with('"') {
            parse_double_quoted_string(inner, line)?
        } else if inner.starts_with('\'') {
            parse_single_quoted_string(inner, line)?
        } else {
            return Err(ExtensionError::TomlParse {
                line,
                message: format!("expected quoted string inside array near '{}'", inner),
            });
        };
        items.push(item);

        let after_item = rest.trim_start();
        if after_item.is_empty() {
            break;
        }
        if let Some(after_comma) = after_item.strip_prefix(',') {
            inner = after_comma.trim_start();
        } else {
            return Err(ExtensionError::TomlParse {
                line,
                message: format!("expected ',' between array elements near '{}'", after_item),
            });
        }
    }

    Ok(items)
}

fn parse_toml_integer(s: &str) -> Option<i64> {
    if s.is_empty() || s.starts_with('_') || s.ends_with('_') || s.contains("__") {
        return None;
    }
    let cleaned = s.replace('_', "");
    cleaned.parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "ruhi_ext_test_{}_{}_{}",
                prefix,
                std::process::id(),
                nanos
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, content).expect("write test file");
    }

    #[test]
    fn parses_all_three_fsm_archetypes_and_full_manifest() {
        let syllabic = r#"
# Syllabic combinator extension manifest
id = "vi-telex"
name = "Tiếng Việt Telex"
version = "0.2.0"
language_code = "vi-VN"
archetype = "syllabic_combinator"
author = "RuhOS Community"
description = "Vietnamese Telex syllabic input bundle"
license = "Apache-2.0"
min_core_version = "0.9.6"
tags = [
  "vietnamese",
  "telex", # inline comment inside multiline array
  "southeast-asia",
]

[layers]
keymap = "keymap.toml"
lexicon = "lexicon.tsv"
morphology = "morphology.toml"
phonetics = "phonetics.toml"
patterns = "patterns.jsonl"

[ui]
indicator = "VI"
display_name = "越語 Telex"
capslock_switch = "none"

[fsm]
max_buffer_len = 32
tab_prefix_completion = true
passthrough_identifiers = false
"#;

        let m1 = parse_manifest_toml(syllabic).expect("parse syllabic manifest");
        assert_eq!(m1.id, "vi-telex");
        assert_eq!(m1.bcp47(), "vi-VN");
        assert_eq!(m1.archetype, FsmArchetype::SyllabicCombinator);
        assert_eq!(m1.archetype.as_str(), "syllabic_combinator");
        assert_eq!(m1.tags, vec!["vietnamese", "telex", "southeast-asia"]);
        assert_eq!(m1.keymap.as_deref(), Some("keymap.toml"));
        assert_eq!(m1.lexicon.as_deref(), Some("lexicon.tsv"));
        assert_eq!(m1.morphology.as_deref(), Some("morphology.toml"));
        assert_eq!(m1.phonetics.as_deref(), Some("phonetics.toml"));
        assert_eq!(m1.patterns.as_deref(), Some("patterns.jsonl"));
        assert_eq!(m1.ui.indicator.as_deref(), Some("VI"));
        assert_eq!(m1.ui.capslock_switch.as_deref(), Some("none"));
        assert_eq!(m1.fsm.max_buffer_len, Some(32));
        assert_eq!(m1.fsm.tab_prefix_completion, Some(true));
        assert_eq!(m1.fsm.passthrough_identifiers, Some(false));

        let code_table = r#"
id = "wubi-98"
name = "五筆 98"
version = "1.0.0"
bcp47 = "zh-Hant"
archetype = "CodeTableShape"
keymap = 'keymap.toml'
"#;
        let m2 = parse_manifest_toml(code_table).expect("parse code table manifest");
        assert_eq!(m2.language_code, "zh-Hant");
        assert_eq!(m2.archetype, FsmArchetype::CodeTableShape);
        assert_eq!(m2.archetype.as_str(), "code_table_shape");
        assert_eq!(m2.keymap.as_deref(), Some("keymap.toml"));

        let tokenizing = r#"
id = "fr-azerty"
name = "Français AZERTY"
version = "0.1.0"
language_code = "fr-FR"
archetype = "tokenizing-lexeme"
"#;
        let m3 = parse_manifest_toml(tokenizing).expect("parse tokenizing manifest");
        assert_eq!(m3.archetype, FsmArchetype::TokenizingLexeme);
        assert_eq!(m3.archetype.as_str(), "tokenizing_lexeme");
    }

    #[test]
    fn rejects_missing_required_fields() {
        let missing_id = r#"
name = "Test"
version = "0.1.0"
language_code = "en-US"
archetype = "tokenizing_lexeme"
"#;
        let err = parse_manifest_toml(missing_id).expect_err("must reject missing id");
        assert_eq!(
            err,
            ExtensionError::MissingRequiredField {
                field: "id".to_string()
            }
        );

        let missing_lang = r#"
id = "test-ext"
name = "Test"
version = "0.1.0"
archetype = "tokenizing_lexeme"
"#;
        let err2 = parse_manifest_toml(missing_lang).expect_err("must reject missing language_code");
        assert_eq!(
            err2,
            ExtensionError::MissingRequiredField {
                field: "language_code".to_string()
            }
        );
    }

    #[test]
    fn rejects_invalid_archetype_and_reports_toml_line_numbers() {
        let bad_archetype = r#"
id = "es-qwerty"
name = "Español"
version = "0.1.0"
language_code = "es-ES"
archetype = "neural_magic"
"#;
        let err = parse_manifest_toml(bad_archetype).expect_err("must reject unknown archetype");
        assert_eq!(
            err,
            ExtensionError::InvalidArchetype {
                value: "neural_magic".to_string()
            }
        );

        let bad_syntax = "id = \"ok\"\nname = \"unterminated\nversion = \"1.0\"";
        let err_syntax = parse_manifest_toml(bad_syntax).expect_err("must report line error");
        match err_syntax {
            ExtensionError::TomlParse { line, .. } => assert_eq!(line, 2),
            other => panic!("expected TomlParse, got {:?}", other),
        }
    }

    #[test]
    fn rejects_path_traversal_and_absolute_paths() {
        let temp = TempDirGuard::new("traversal");
        let bundle = temp.path.join("fr-azerty");
        write_file(
            &bundle.join("manifest.toml"),
            r#"
id = "fr-azerty"
name = "Français AZERTY"
version = "0.1.0"
language_code = "fr-FR"
archetype = "tokenizing_lexeme"
keymap = "../secret.toml"
"#,
        );
        write_file(&temp.path.join("secret.toml"), "secret = true\n");

        let err = load_extension_bundle(&bundle).expect_err("must reject .. traversal");
        assert_eq!(
            err,
            ExtensionError::PathTraversal {
                field: "keymap".to_string(),
                declared_path: "../secret.toml".to_string()
            }
        );

        let nested_escape = temp.path.join("nested-escape");
        write_file(
            &nested_escape.join("manifest.toml"),
            r#"
id = "nested-escape"
name = "Nested Escape"
version = "0.1.0"
language_code = "de-DE"
archetype = "tokenizing_lexeme"
lexicon = "sub/../../secret.toml"
"#,
        );
        let err2 = load_extension_bundle(&nested_escape).expect_err("must reject nested ..");
        assert_eq!(
            err2,
            ExtensionError::PathTraversal {
                field: "lexicon".to_string(),
                declared_path: "sub/../../secret.toml".to_string()
            }
        );
    }

    #[test]
    fn detects_missing_declared_asset_error_and_undeclared_layer_warnings() {
        let temp = TempDirGuard::new("assets");
        let missing_asset_dir = temp.path.join("broken-bundle");
        write_file(
            &missing_asset_dir.join("manifest.toml"),
            r#"
id = "de-qwertz"
name = "Deutsch QWERTZ"
version = "0.1.0"
language_code = "de-DE"
archetype = "tokenizing_lexeme"
keymap = "keymap.toml"
lexicon = "data/lexicon.tsv"
"#,
        );
        write_file(&missing_asset_dir.join("keymap.toml"), "# keymap\n");

        let err = load_extension_bundle(&missing_asset_dir)
            .expect_err("must error when declared lexicon is missing");
        match err {
            ExtensionError::MissingAsset { field, path } => {
                assert_eq!(field, "lexicon");
                assert!(path.ends_with(Path::new("data").join("lexicon.tsv")));
            }
            other => panic!("expected MissingAsset, got {:?}", other),
        }

        // Minimal bundle without any declared layers loads with structured warnings.
        let minimal_dir = temp.path.join("minimal-bundle");
        write_file(
            &minimal_dir.join("manifest.toml"),
            r#"
id = "minimal-test"
name = "Minimal"
version = "0.1.0"
language_code = "en-US"
archetype = "tokenizing_lexeme"
"#,
        );
        let loaded = load_extension_bundle(&minimal_dir).expect("minimal bundle loads");
        assert!(loaded.diagnostics.has_warnings());
        assert_eq!(loaded.diagnostics.warnings.len(), 2);
    }

    #[test]
    fn discovers_extensions_across_multiple_roots_in_deterministic_order() {
        let temp = TempDirGuard::new("discover");
        let missing_root = temp.path.join("does_not_exist");
        let root_a = temp.path.join("root_a");
        let root_b = temp.path.join("root_b");

        // README.md and hidden directory inside root_a must be ignored.
        write_file(&root_a.join("README.md"), "# Extensions\n");
        write_file(&root_a.join(".hidden").join("manifest.toml"), "broken");

        write_file(
            &root_a.join("z-korean").join("manifest.toml"),
            r#"
id = "ko-dubeolsik"
name = "Korean 2-Set"
version = "0.1.0"
language_code = "ko-KR"
archetype = "syllabic_combinator"
keymap = "keymap.toml"
"#,
        );
        write_file(&root_a.join("z-korean").join("keymap.toml"), "# ko\n");

        write_file(
            &root_a.join("a-cangjie").join("manifest.toml"),
            r#"
id = "zh-cangjie5"
name = "倉頡五代"
version = "0.1.0"
language_code = "zh-Hant"
archetype = "code_table_shape"
keymap = "keymap.toml"
"#,
        );
        write_file(&root_a.join("a-cangjie").join("keymap.toml"), "# cj\n");

        write_file(
            &root_b.join("b-french").join("manifest.toml"),
            r#"
id = "fr-azerty"
name = "Français AZERTY"
version = "0.1.0"
language_code = "fr-FR"
archetype = "tokenizing_lexeme"
keymap = "keymap.toml"
"#,
        );
        write_file(&root_b.join("b-french").join("keymap.toml"), "# fr\n");

        // Empty root_empty yields nothing; missing_root yields nothing without error.
        let empty_only = discover_extensions(&[&missing_root]);
        assert!(empty_only.is_empty());

        let discovered = discover_extensions(&[&missing_root, &root_a, &root_b]);
        assert_eq!(discovered.len(), 3);

        let ids: Vec<String> = discovered
            .into_iter()
            .map(|res| res.expect("valid bundle").manifest.id)
            .collect();
        // root_a sorted: a-cangjie, z-korean; then root_b sorted: b-french
        assert_eq!(ids, vec!["zh-cangjie5", "ko-dubeolsik", "fr-azerty"]);
    }
}
