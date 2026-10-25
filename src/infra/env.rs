//! Nullable wrapper over environment variables.

#[cfg(test)]
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Env {
    inner: Inner,
}

#[derive(Debug, Clone)]
enum Inner {
    Real,
    #[cfg(test)]
    Null(HashMap<String, String>),
}

impl Env {
    #[must_use]
    pub fn create() -> Self {
        Env { inner: Inner::Real }
    }

    #[must_use]
    pub fn var(&self, name: &str) -> Option<String> {
        match &self.inner {
            Inner::Real => std::env::var(name).ok(),
            #[cfg(test)]
            Inner::Null(vars) => vars.get(name).cloned(),
        }
    }
}

#[cfg(test)]
impl Env {
    /// An environment containing exactly the given variables. Bare
    /// `create_null([])` is an empty environment.
    pub fn create_null(
        vars: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Env {
            inner: Inner::Null(
                vars.into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn nulled_env_returns_configured_vars_only() {
        let env = Env::create_null([("VIRTUAL_ENV", "/venvs/demo")]);
        assert_eq!(env.var("VIRTUAL_ENV"), Some("/venvs/demo".to_string()));
        assert_eq!(env.var("UNSET"), None);
    }

    #[test]
    fn real_env_reads_the_process_environment() {
        // PATH is set in any environment that can run this test.
        assert!(Env::create().var("PATH").is_some());
    }
}
