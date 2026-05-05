#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneratedStringKind {
    Keyframes,
    PositionTry,
    ViewTransitionClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GeneratedString {
    pub value: String,
    pub kind: GeneratedStringKind,
}

impl GeneratedString {
    pub(crate) fn new(value: String, kind: GeneratedStringKind) -> Self {
        Self { value, kind }
    }

    pub(crate) fn is_css_identifier(&self) -> bool {
        matches!(
            self.kind,
            GeneratedStringKind::Keyframes | GeneratedStringKind::PositionTry
        )
    }
}
