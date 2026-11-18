use std::path::PathBuf;
#[derive(Clone, Debug)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: u64,
    pub executable: PathBuf,
}
impl ProcessIdentity {
    pub fn capture(pid: u32) -> Option<Self> {
        let proc = PathBuf::from(format!("/proc/{pid}"));
        let stat = std::fs::read_to_string(proc.join("stat")).ok()?;
        let start_time = stat
            .rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(19)?
            .parse()
            .ok()?;
        Some(Self {
            pid,
            start_time,
            executable: std::fs::read_link(proc.join("exe")).ok()?,
        })
    }
    pub fn owns_descendant(&self, mut pid: u32) -> bool {
        if !self.still_owned() {
            return false;
        }
        for _ in 0..64 {
            if pid == self.pid {
                return self.still_owned();
            }
            let Some(identity) = Self::capture(pid) else {
                return false;
            };
            if identity.start_time < self.start_time {
                return false;
            }
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return false;
            };
            let Some(parent) = stat
                .rsplit_once(')')
                .and_then(|(_, tail)| tail.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u32>().ok())
            else {
                return false;
            };
            if parent <= 1 || parent == pid {
                return false;
            }
            pid = parent;
        }
        false
    }
    pub fn still_owned(&self) -> bool {
        Self::capture(self.pid).is_some_and(|current| {
            current.start_time == self.start_time && current.executable == self.executable
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pid_reuse_is_not_treated_as_owned() {
        let mut identity = ProcessIdentity::capture(std::process::id()).unwrap();
        assert!(identity.still_owned());
        identity.start_time += 1;
        assert!(!identity.still_owned());
    }
}
