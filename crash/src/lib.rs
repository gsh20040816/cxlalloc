use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::panic::UnwindSafe;

thread_local! {
    #[doc(hidden)]
    pub static LOCAL: RefCell<Option<Dynamic>> = const { RefCell::new(None) };
}

#[doc(hidden)]
pub use ::linkme;

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Function(&'static str);

impl Function {
    #[doc(hidden)]
    pub fn new<T>(function: &T) -> Self {
        Self(std::any::type_name_of_val(function))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Static(&'static str);

impl Static {
    #[doc(hidden)]
    pub const fn new(identifier: &'static str) -> Self {
        Self(identifier)
    }
}

/// A dynamic crash point is a static crash point
/// that optionally filters via runtime stack trace.
///
/// Necessary for crashing in library code that can
/// be called from different code paths.
#[derive(Clone, PartialEq, Eq)]
pub struct Dynamic {
    stack: Vec<Function>,
    point: Static,
}

impl fmt::Debug for Dynamic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut stack = self.stack.iter();

        let Some(Function(head)) = stack.next() else {
            return write!(f, "{}", self.point.0);
        };

        write!(f, "[{head}")?;
        for Function(tail) in stack {
            write!(f, ", {tail}")?;
        }
        write!(f, "]: {}", self.point.0)
    }
}

impl Dynamic {
    #[doc(hidden)]
    pub const fn new(stack: Vec<Function>, point: Static) -> Self {
        Self { stack, point }
    }

    #[doc(hidden)]
    pub fn matches(&self, point: Static) -> bool {
        // Short-circuit: crash point mismatch
        if self.point != point {
            return false;
        }

        // Short-circuit: no stack frame filter
        if self.stack.is_empty() {
            return true;
        }

        let mut stack = self.stack.iter().rev().peekable();
        let mut buffer = String::new();

        backtrace::trace(|frame| {
            // Stop unwinding if we've run out of functions to match
            if stack.peek().is_none() {
                return false;
            }

            backtrace::resolve_frame(frame, |symbol| {
                let Some(name) = symbol.name() else { return };

                // Need to use `Display` to demangle
                buffer.clear();
                use std::fmt::Write as _;
                write!(&mut buffer, "{name}").unwrap();
            });

            stack.next_if(|Function(next)| buffer.starts_with(next));
            true
        });

        stack.count() == 0
    }
}

#[doc(hidden)]
#[linkme::distributed_slice]
pub static COVERED: [Static] = [..];

#[doc(hidden)]
#[linkme::distributed_slice]
pub static DEFINED: [Static] = [..];

/// Create a dynamic reference to a static crash point.
#[macro_export]
macro_rules! reference {
    ($([ $($function:path),* $(,)? ]:)? $point:ident) => {{
        #[allow(non_upper_case_globals)]
        #[$crate::linkme::distributed_slice($crate::COVERED)]
        #[linkme(crate = $crate::linkme)]
        static $point: $crate::Static = $crate::Static::new(stringify!($point));

        $crate::Dynamic::new(
            vec![
                $(
                    $(
                        $crate::Function::new(&$function)
                    ),*
                )?
            ],
            $point,
        )
    }};
}

/// Define a static crash point.
#[macro_export]
macro_rules! define {
    ($point:ident) => {{
        #[allow(non_upper_case_globals)]
        #[$crate::linkme::distributed_slice($crate::DEFINED)]
        #[linkme(crate = $crate::linkme)]
        static $point: $crate::Static = $crate::Static::new(stringify!($point));

        $crate::LOCAL.with_borrow(|dynamic| match dynamic {
            Some(dynamic) if dynamic.matches($point) => std::panic::panic_any(dynamic.clone()),
            Some(_) | None => (),
        })
    }};
}

pub fn run<F: FnOnce() + UnwindSafe>(crash: Dynamic, closure: F) {
    let expected = crash.clone();
    LOCAL.set(Some(crash));

    let payload = match std::panic::catch_unwind(closure) {
        Ok(()) => panic!("Expected crash at {expected:?}, but did not crash"),
        Err(payload) => payload,
    };

    match payload.downcast::<Dynamic>() {
        Ok(actual) if *actual == expected => (),
        Ok(actual) => panic!("Expected crash at {expected:?}, but crashed at {actual:?}",),
        Err(actual) => std::panic::resume_unwind(actual),
    }

    LOCAL.set(None);
}

/// Assert that the set of covered and defined crash points are the same.
pub fn assert_coverage() {
    let defined = DEFINED.iter().copied().collect::<HashSet<_>>();
    let covered = COVERED.iter().copied().collect::<HashSet<_>>();

    let mut uncovered = defined.difference(&covered).copied().collect::<Vec<_>>();
    uncovered.sort();

    let mut undefined = covered.difference(&defined).copied().collect::<Vec<_>>();
    undefined.sort();

    const EMPTY: &[Static] = &[];

    assert_eq!(
        uncovered, EMPTY,
        "Set of defined but uncovered crash points should be empty"
    );

    assert_eq!(
        undefined, EMPTY,
        "Set of covered but undefined crash points should be empty"
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn coverage() {
        super::assert_coverage()
    }

    fn a() {
        super::define!(crash_a_0);
        super::define!(crash_a_1);
    }

    #[test]
    fn no_stack_0() {
        super::run(super::reference!(crash_a_0), a);
    }

    #[test]
    fn no_stack_1() {
        super::run(super::reference!(crash_a_1), a);
    }

    fn b() {
        a();
    }

    fn c() {
        a();
    }

    #[test]
    fn stack_match_parent() {
        super::run(super::reference!([b]: crash_a_1), b);
    }

    #[test]
    #[should_panic(expected = "did not crash")]
    fn stack_no_match() {
        super::run(super::reference!([b]: crash_a_1), c);
    }

    fn d() {
        b();
    }

    #[test]
    fn stack_match_skip_one() {
        super::run(super::reference!([d]: crash_a_1), d);
    }

    #[test]
    fn stack_match_self() {
        super::run(super::reference!([stack_match_self]: crash_a_1), d);
    }

    #[test]
    fn stack_match_self_path() {
        use crate::tests as foo;
        super::run(super::reference!([foo::stack_match_self]: crash_a_1), d);
    }

    #[test]
    fn stack_match_all() {
        super::run(super::reference!([stack_match_all, d, b, a]: crash_a_1), d);
    }
}
