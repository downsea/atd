use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Endpoint {
    UnixSocket(PathBuf),
}

impl Endpoint {
    pub fn unix(path: impl Into<PathBuf>) -> Self {
        Endpoint::UnixSocket(path.into())
    }

    /// Default ANOS daemon socket: `$HOME/.anos/anos.sock`.
    pub fn default_anos() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Endpoint::UnixSocket(PathBuf::from(home).join(".anos").join("anos.sock"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_constructor_accepts_str_and_pathbuf() {
        let e1 = Endpoint::unix("/tmp/a.sock");
        let e2 = Endpoint::unix(PathBuf::from("/tmp/a.sock"));
        match (e1, e2) {
            (Endpoint::UnixSocket(a), Endpoint::UnixSocket(b)) => assert_eq!(a, b),
        }
    }

    #[test]
    fn default_anos_uses_dot_anos_anos_sock() {
        let e = Endpoint::default_anos();
        match e {
            Endpoint::UnixSocket(p) => {
                let s = p.to_string_lossy().to_string();
                assert!(s.ends_with(".anos/anos.sock"), "got: {s}");
            }
        }
    }
}
