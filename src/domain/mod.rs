use crate::error::AstraError;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchRepresentation {
    Original,
    Summary,
    SyntheticQuestion,
    KeyFact,
    Entity,
    Term,
    Definition,
    Faq,
}
impl SearchRepresentation {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Original => "ORIGINAL",
            Self::Summary => "SUMMARY",
            Self::SyntheticQuestion => "SYNTHETIC_QUESTION",
            Self::KeyFact => "KEY_FACT",
            Self::Entity => "ENTITY",
            Self::Term => "TERM",
            Self::Definition => "DEFINITION",
            Self::Faq => "FAQ",
        }
    }
    pub fn parse(v: &str) -> Result<Self, AstraError> {
        match v {
            "ORIGINAL" => Ok(Self::Original),
            "SUMMARY" => Ok(Self::Summary),
            "SYNTHETIC_QUESTION" => Ok(Self::SyntheticQuestion),
            "KEY_FACT" => Ok(Self::KeyFact),
            "ENTITY" => Ok(Self::Entity),
            "TERM" => Ok(Self::Term),
            "DEFINITION" => Ok(Self::Definition),
            "FAQ" => Ok(Self::Faq),
            _ => Err(AstraError::InvalidArgument(format!(
                "unknown representation {v}"
            ))),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum TtlUpdateMode {
    ReplaceFromNow,
    ExtendFromCurrent,
    KeepLongest,
    RemoveTtl,
}
