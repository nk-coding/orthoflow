// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/invariant/errors.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::{GoError, Sentinel};

pub static ErrViolation: Sentinel = Sentinel::new("layout invariant violated");

/// `invariant.New(reason)`: `fmt.Errorf("%w: %s", ErrViolation, reason)`.
pub fn New(reason: impl AsRef<str>) -> GoError {
    GoError::wrap(format!("{}: {}", ErrViolation, reason.as_ref()), ErrViolation.err())
}

/// `invariant.Errorf(format, args...)`: pass the already formatted message
/// (`invariant::Errorf(format!("node {} has no owner", 42))`) or use [`errorf!`].
pub fn Errorf(msg: impl AsRef<str>) -> GoError {
    New(msg)
}

/// `invariant.Errorf("fmt", args...)` → `invariant::errorf!("fmt", args...)`
#[macro_export]
macro_rules! errorf {
    ($($arg:tt)*) => { $crate::New(format!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn TestNew() {
        let err = New("broken ownership");
        assert_eq!(err.Error(), "layout invariant violated: broken ownership");
        assert!(err.Is(&ErrViolation));
    }
    #[test]
    fn TestErrorf() {
        let err = crate::errorf!("node {} has no owner", 42);
        assert_eq!(err.Error(), "layout invariant violated: node 42 has no owner");
        assert!(err.Is(&ErrViolation));
        let w = GoError::wrap(format!("ctx: {}", err), err.clone());
        assert!(w.Is(&ErrViolation));
    }
}
