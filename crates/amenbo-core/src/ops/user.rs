//! Assignee/author resolution. There is only one kind of actor — the facet (human / ai) — so
//! resolving an assignee or author token down to a facet is [`crate::config::Config::resolve_facet`]'s
//! job (it matches against the two display names in config plus the reserved words), and
//! [`crate::config::Config::roster`] derives the roster from those same two names. All that is left
//! here is the noun used in not_found messages (the English/Japanese pair).

use crate::error::ErrorCode;
use crate::ops::Noun;

/// This entity's noun (the English/Japanese pair used in not_found messages).
pub(crate) const NOUN: Noun = Noun { en: "user", ja: "ユーザー", code: ErrorCode::NotFoundUser };
