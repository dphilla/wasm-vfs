use core::fmt;                 // for Debug
use core::hash::{Hash, Hasher};
use serde::Serialize;          // if you’re using Serde for serialization

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct PathBuf {
    inner: String,
}

impl Hash for PathBuf {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

// Optionally implement Debug (or derive it) so you can do #[derive(Debug)]:
impl fmt::Debug for PathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathBuf")
         .field("inner", &self.inner)
         .finish()
    }
}

// Provide minimal methods: as_path(), file_name(), is_absolute(), join(), etc.
impl PathBuf {
    pub fn new() -> Self {
        Self { inner: String::new() }
    }

    pub fn from<S: Into<String>>(s: S) -> Self {
        Self { inner: s.into() }
    }

    pub fn as_path(&self) -> &Self {
        // A trivial "fake" method.
        // Real code might return a &Path or something similar if you used std::path.
        self
    }

    pub fn is_absolute(&self) -> bool {
        self.inner.starts_with('/')
    }

    pub fn join(&self, other: &PathBuf) -> PathBuf {
        if other.is_absolute() {
            other.clone()
        } else {
            let mut joined = self.inner.clone();
            if !joined.ends_with('/') && !joined.is_empty() {
                joined.push('/');
            }
            joined.push_str(&other.inner);
            PathBuf { inner: joined }
        }
    }

    pub fn file_name(&self) -> Option<&str> {
        if self.inner.is_empty() {
            return None;
        }
        let trimmed = self.inner.trim_end_matches('/');
        trimmed.rsplit_once('/')
               .map(|(_, filename)| filename)
               .or_else(|| Some(trimmed))
    }

    pub fn parent(&self) -> Option<Self> {
        if self.inner == "/" {
            return None;
        }
        let trimmed = self.inner.trim_end_matches('/');
        if trimmed.is_empty() {
            return Some(PathBuf::from("/"));
        }
        let idx = trimmed.rfind('/')?;
        if idx == 0 {
            Some(PathBuf::from("/"))
        } else {
            Some(PathBuf::from(&trimmed[..idx]))
        }
    }

    /// Minimal version of "lossy" for debug or string conversion
    pub fn to_string_lossy(&self) -> String {
        // In real code, you'd handle UTF-8 properly. We'll do a direct clone:
        self.inner.clone()
    }
}

