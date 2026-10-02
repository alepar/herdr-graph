//! Crash-injection failpoints (feature `test-support`). Expanded to nothing in normal builds.
//! hg-zmi.3 places `failpoint!("writer.after_tree_build")` etc. at writer boundaries.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailAction {
    Panic,
    Exit(i32),
}

#[cfg(feature = "test-support")]
mod imp {
    use super::FailAction;
    use std::collections::BTreeMap;
    use std::sync::{Mutex, OnceLock};
    fn registry() -> &'static Mutex<BTreeMap<String, FailAction>> {
        static R: OnceLock<Mutex<BTreeMap<String, FailAction>>> = OnceLock::new();
        R.get_or_init(Default::default)
    }
    pub fn arm(name: &str, action: FailAction) {
        registry().lock().unwrap().insert(name.to_owned(), action);
    }
    pub fn disarm(name: &str) {
        registry().lock().unwrap().remove(name);
    }
    /// Arm from env `HG_FAILPOINTS="a=panic,b=exit:3"` (for subprocess crash tests).
    pub fn arm_from_env() {
        let Ok(spec) = std::env::var("HG_FAILPOINTS") else {
            return;
        };
        for item in spec.split(',').filter(|s| !s.is_empty()) {
            if let Some((n, a)) = item.split_once('=') {
                let action = match a {
                    "panic" => Some(FailAction::Panic),
                    _ => a
                        .strip_prefix("exit:")
                        .and_then(|c| c.parse().ok())
                        .map(FailAction::Exit),
                };
                if let Some(action) = action {
                    arm(n, action);
                }
            }
        }
    }
    pub fn trigger(name: &str) {
        let action = registry().lock().unwrap().get(name).copied();
        match action {
            Some(FailAction::Panic) => panic!("failpoint {name} hit"),
            Some(FailAction::Exit(code)) => std::process::exit(code),
            None => {}
        }
    }
}
#[cfg(feature = "test-support")]
pub use imp::{arm, arm_from_env, disarm, trigger};

/// `failpoint!("name")`: no-op unless built with `test-support` and the name is armed.
#[macro_export]
macro_rules! failpoint {
    ($name:expr) => {{
        #[cfg(feature = "test-support")]
        {
            $crate::failpoint::trigger($name);
        }
    }};
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use super::*;
    #[test]
    fn unarmed_failpoint_is_noop() {
        crate::failpoint!("fp.test.unarmed");
    }
    #[test]
    fn armed_panic_failpoint_panics() {
        arm("fp.test.panic", FailAction::Panic);
        let r = std::panic::catch_unwind(|| crate::failpoint!("fp.test.panic"));
        disarm("fp.test.panic");
        assert!(r.is_err());
    }
}

#[cfg(all(test, not(feature = "test-support")))]
mod default_tests {
    #[test]
    fn failpoint_compiles_out() {
        crate::failpoint!("anything");
    }
}
