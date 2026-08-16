//! Stable application identity contracts shared across SOL services.
//!
//! [`AppId`] is the durable, machine-readable key for an application.  It is
//! deliberately separate from a package name, install location, desktop-file
//! path, and release version: those are deployment details that can change
//! without changing which application owns a window, command, notification, or
//! store listing.

use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// The maximum number of UTF-8 bytes in an [`AppId`].
pub const APP_ID_MAX_LENGTH: usize = 255;

/// A validated, reverse-DNS application identifier.
///
/// Its canonical spelling is lowercase ASCII and has two or more dot-separated
/// components.  Every component starts with a letter, ends with a letter or
/// digit, and may contain lowercase letters, digits, or hyphens in between.
/// For example, `org.sol.files` and `io.example.photo-editor` are valid.
///
/// An app ID is the durable join key for launchers, notification attribution,
/// command ownership, and store records.  It is not a package name, D-Bus
/// name, desktop-file filename, URI scheme, or release version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppId(String);

impl AppId {
    /// Parse and validate an application identifier.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, AppIdError> {
        let value = value.as_ref();
        if value.is_empty() {
            return Err(AppIdError::Empty);
        }
        if value.len() > APP_ID_MAX_LENGTH {
            return Err(AppIdError::TooLong {
                max_length: APP_ID_MAX_LENGTH,
            });
        }

        let components: Vec<_> = value.split('.').collect();
        if components.len() < 2 {
            return Err(AppIdError::MissingNamespace);
        }

        for (index, component) in components.iter().enumerate() {
            if component.is_empty() {
                return Err(AppIdError::EmptyComponent { index });
            }

            let mut characters = component.char_indices();
            let (_, first) = characters.next().expect("empty components rejected above");
            if !first.is_ascii_lowercase() {
                return Err(AppIdError::ComponentMustStartWithLetter { index });
            }

            let mut last = first;
            for (offset, character) in characters {
                if !(character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || character == '-')
                {
                    return Err(AppIdError::InvalidCharacter {
                        index,
                        character,
                        byte_offset: offset,
                    });
                }
                last = character;
            }

            if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
                return Err(AppIdError::ComponentMustEndWithAlphanumeric { index });
            }
        }

        Ok(Self(value.to_owned()))
    }

    /// Return the canonical identifier string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for AppId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AppId {
    type Err = AppIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for AppId {
    type Error = AppIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<String> for AppId {
    type Error = AppIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

/// The reason an [`AppId`] was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppIdError {
    /// The identifier is empty.
    Empty,
    /// The identifier is longer than [`APP_ID_MAX_LENGTH`].
    TooLong {
        /// The permitted maximum, in UTF-8 bytes.
        max_length: usize,
    },
    /// The identifier does not have both a namespace and application component.
    MissingNamespace,
    /// A dot-separated component is empty.
    EmptyComponent {
        /// The zero-based component position.
        index: usize,
    },
    /// A component does not start with a lowercase ASCII letter.
    ComponentMustStartWithLetter {
        /// The zero-based component position.
        index: usize,
    },
    /// A component does not end with a lowercase ASCII letter or digit.
    ComponentMustEndWithAlphanumeric {
        /// The zero-based component position.
        index: usize,
    },
    /// A component contains a character outside the canonical grammar.
    InvalidCharacter {
        /// The zero-based component position.
        index: usize,
        /// The rejected character.
        character: char,
        /// Its byte offset within the component.
        byte_offset: usize,
    },
}

impl fmt::Display for AppIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("application ID must not be empty"),
            Self::TooLong { max_length } => {
                write!(
                    formatter,
                    "application ID must not exceed {max_length} bytes"
                )
            }
            Self::MissingNamespace => formatter.write_str(
                "application ID must contain at least two dot-separated reverse-DNS components",
            ),
            Self::EmptyComponent { index } => {
                write!(
                    formatter,
                    "application ID component {index} must not be empty"
                )
            }
            Self::ComponentMustStartWithLetter { index } => write!(
                formatter,
                "application ID component {index} must start with a lowercase ASCII letter"
            ),
            Self::ComponentMustEndWithAlphanumeric { index } => write!(
                formatter,
                "application ID component {index} must end with a lowercase ASCII letter or digit"
            ),
            Self::InvalidCharacter {
                index, character, ..
            } => write!(
                formatter,
                "application ID component {index} contains invalid character {character:?}"
            ),
        }
    }
}

impl Error for AppIdError {}

/// Shared, user-facing identity metadata for an application.
///
/// Launcher and notification surfaces use this type when they need a display
/// name in addition to the durable [`AppId`].  Store-specific release data and
/// frontend-specific icon representations intentionally remain outside this
/// contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    app_id: AppId,
    display_name: String,
}

impl AppIdentity {
    /// Create identity metadata with a non-empty, user-facing display name.
    pub fn new(app_id: AppId, display_name: impl Into<String>) -> Result<Self, AppIdentityError> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(AppIdentityError::EmptyDisplayName);
        }

        Ok(Self {
            app_id,
            display_name,
        })
    }

    /// Return the durable application ID.
    #[must_use]
    pub fn app_id(&self) -> &AppId {
        &self.app_id
    }

    /// Return the user-facing application name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

/// The reason [`AppIdentity`] metadata was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppIdentityError {
    /// The user-facing name is empty or whitespace-only.
    EmptyDisplayName,
}

impl fmt::Display for AppIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDisplayName => {
                formatter.write_str("application display name must not be empty")
            }
        }
    }
}

impl Error for AppIdentityError {}

#[cfg(test)]
mod tests {
    use super::{APP_ID_MAX_LENGTH, AppId, AppIdError, AppIdentity, AppIdentityError};

    #[test]
    fn parses_canonical_reverse_dns_ids() {
        let id = AppId::parse("org.sol.files").expect("canonical ID should parse");

        assert_eq!(id.as_str(), "org.sol.files");
        assert_eq!(id.to_string(), "org.sol.files");
        assert_eq!(id.as_ref(), "org.sol.files");
    }

    #[test]
    fn accepts_hyphens_only_inside_components() {
        assert!(AppId::parse("io.example.photo-editor").is_ok());
        assert!(AppId::parse("io.example.photo-2").is_ok());
    }

    #[test]
    fn rejects_noncanonical_ids_with_specific_errors() {
        assert_eq!(AppId::parse(""), Err(AppIdError::Empty));
        assert_eq!(AppId::parse("sol"), Err(AppIdError::MissingNamespace));
        assert_eq!(
            AppId::parse("org..sol"),
            Err(AppIdError::EmptyComponent { index: 1 })
        );
        assert_eq!(
            AppId::parse("org.SOL.files"),
            Err(AppIdError::ComponentMustStartWithLetter { index: 1 })
        );
        assert_eq!(
            AppId::parse("org.sol.files-"),
            Err(AppIdError::ComponentMustEndWithAlphanumeric { index: 2 })
        );
        assert!(matches!(
            AppId::parse("org.sol.file_name"),
            Err(AppIdError::InvalidCharacter {
                index: 2,
                character: '_',
                ..
            })
        ));
    }

    #[test]
    fn rejects_ids_longer_than_the_contract_limit() {
        let value = format!("a.{}", "b".repeat(APP_ID_MAX_LENGTH - 1));

        assert_eq!(
            AppId::parse(value),
            Err(AppIdError::TooLong {
                max_length: APP_ID_MAX_LENGTH
            })
        );
    }

    #[test]
    fn identity_keeps_display_metadata_separate_from_app_id() {
        let id = AppId::parse("org.sol.files").expect("ID should parse");
        let identity = AppIdentity::new(id.clone(), "Files").expect("name should be valid");

        assert_eq!(identity.app_id(), &id);
        assert_eq!(identity.display_name(), "Files");
        assert_eq!(
            AppIdentity::new(id, "   "),
            Err(AppIdentityError::EmptyDisplayName)
        );
    }
}
