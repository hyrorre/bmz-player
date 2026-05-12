#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrangeOption {
    #[default]
    Normal,
    Mirror,
    Random,
}

impl ArrangeOption {
    pub fn cycle(self) -> Self {
        match self {
            Self::Normal => Self::Mirror,
            Self::Mirror => Self::Random,
            Self::Random => Self::Normal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Mirror => "MIRROR",
            Self::Random => "RANDOM",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssistOption {
    #[default]
    Normal,
    Autoplay,
}

impl AssistOption {
    pub fn cycle(self) -> Self {
        match self {
            Self::Normal => Self::Autoplay,
            Self::Autoplay => Self::Normal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Autoplay => "AUTOPLAY",
        }
    }
}
