//! Compiling and running C++ with the machine's own compiler.
//!
//! This is a setter's quick check, not the reproducible Phase 3 build (wasm + fuel ticks):
//! times are wall-clock on this machine. Programs run in a scratch directory with a time
//! limit and an output cap; they are the model's code, so the setter starts each run.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::Error;

/// `bits/stdc++.h` is libstdc++-only; Apple clang (libc++) needs this shim.
const STDCXX_SHIM: &str = "#pragma once\n\
#include <algorithm>\n#include <array>\n#include <bitset>\n#include <cassert>\n#include <cctype>\n#include <chrono>\n\
#include <climits>\n#include <cmath>\n#include <complex>\n#include <cstdint>\n#include <cstdio>\n#include <cstdlib>\n\
#include <cstring>\n#include <deque>\n#include <functional>\n#include <iomanip>\n#include <iostream>\n#include <iterator>\n\
#include <limits>\n#include <list>\n#include <map>\n#include <memory>\n#include <numeric>\n#include <optional>\n\
#include <queue>\n#include <random>\n#include <set>\n#include <sstream>\n#include <stack>\n#include <string>\n\
#include <tuple>\n#include <type_traits>\n#include <unordered_map>\n#include <unordered_set>\n#include <utility>\n\
#include <variant>\n#include <vector>\n";

const OUTPUT_CAP: usize = 64 << 20;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Toolchain {
    pub path: PathBuf,
    /// First line of `--version`.
    pub version: String,
}

/// Finds a working C++ compiler: `$XCPC_CXX`, then `clang++`, `g++`, `c++`.
///
/// On macOS `/usr/bin/clang++` exists even without the Command Line Tools and only pops
/// up an installer, so a compiler counts only if `--version` succeeds.
pub fn detect() -> Result<Toolchain, Error> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(cxx) = std::env::var_os("XCPC_CXX").filter(|v| !v.is_empty()) {
        candidates.push(cxx.into());
    }
    candidates.extend(["clang++", "g++", "c++"].map(PathBuf::from));
    for cand in candidates {
        let Ok(out) = Command::new(&cand)
            .arg("--version")
            .stdin(Stdio::null())
            .output()
        else {
            continue;
        };
        if out.status.success() {
            let version = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_owned();
            return Ok(Toolchain {
                path: cand,
                version,
            });
        }
    }
    Err(Error::new(
        "no_compiler",
        if cfg!(target_os = "macos") {
            "没有找到 C++ 编译器。请在「终端」运行 xcode-select --install 安装命令行工具。"
        } else {
            "没有找到 C++ 编译器（clang++ 或 g++）。"
        },
    ))
}

/// A scratch directory for one verification, removed on drop.
pub struct Sandbox {
    pub dir: PathBuf,
}

impl Sandbox {
    pub fn new() -> Result<Self, Error> {
        let dir = std::env::temp_dir().join(format!(
            "xcpc-setter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(dir.join("include/bits")).map_err(|e| Error::new("io", e))?;
        std::fs::write(dir.join("include/bits/stdc++.h"), STDCXX_SHIM)
            .map_err(|e| Error::new("io", e))?;
        Ok(Self { dir })
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Compiles `source` to `<sandbox>/<name>`; on failure returns the compiler's message.
pub fn compile(tc: &Toolchain, sb: &Sandbox, name: &str, source: &str) -> Result<PathBuf, String> {
    let src = sb.dir.join(format!("{name}.cpp"));
    let bin = sb.dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    });
    std::fs::write(&src, source).map_err(|e| e.to_string())?;
    let out = Command::new(&tc.path)
        .args(["-std=c++17", "-O2", "-pipe", "-w"])
        .arg("-I")
        .arg(sb.dir.join("include"))
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(bin)
    } else {
        let log = String::from_utf8_lossy(&out.stderr);
        // The first errors are what matter; the rest is usually a cascade.
        let trimmed: String = log
            .lines()
            .filter(|l| l.contains("error"))
            .take(6)
            .collect::<Vec<_>>()
            .join("\n");
        Err(if trimmed.is_empty() {
            log.chars().take(1500).collect()
        } else {
            trimmed
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Timeout,
    /// Non-zero exit or killed by a signal.
    Crash(String),
    OutputLimit,
}

#[derive(Debug, Clone)]
pub struct Run {
    pub outcome: Outcome,
    pub stdout: String,
    pub stderr: String,
    pub time: Duration,
}

/// Runs `bin args…` with `stdin`, killing it after `limit`.
pub fn run(bin: &Path, args: &[&str], stdin: &str, limit: Duration) -> Run {
    let start = Instant::now();
    let mut child = match Command::new(bin)
        .args(args)
        .current_dir(bin.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return Run {
                outcome: Outcome::Crash(e.to_string()),
                stdout: String::new(),
                stderr: String::new(),
                time: Duration::ZERO,
            };
        }
    };
    let input = stdin.as_bytes().to_vec();
    let mut child_in = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let _ = child_in.write_all(&input);
    });
    let mut out = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut out).take(OUTPUT_CAP as u64 + 1).read_to_end(&mut buf);
        buf
    });
    let mut err = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut err).take(64 << 10).read_to_end(&mut buf);
        buf
    });

    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(_) => break None,
        }
    };
    let time = start.elapsed();
    let _ = writer.join();
    let stdout = reader.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned();
    let outcome = match status {
        None => Outcome::Timeout,
        Some(_) if stdout.len() > OUTPUT_CAP => Outcome::OutputLimit,
        Some(s) if s.success() => Outcome::Ok,
        Some(s) => Outcome::Crash(match s.code() {
            Some(c) => format!("退出码 {c}"),
            None => "被信号终止（可能是段错误或栈溢出）".into(),
        }),
    };
    Run {
        outcome,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr,
        time,
    }
}

/// Token-by-token comparison, the default checker's rule.
pub fn same_tokens(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_runs_and_times_out() {
        let Ok(tc) = detect() else { return };
        let sb = Sandbox::new().unwrap();
        let add = compile(&tc, &sb, "add", "#include <bits/stdc++.h>\nint main(){long long a,b;std::cin>>a>>b;std::cout<<a+b<<\"\\n\";}").unwrap();
        let r = run(&add, &[], "2 3\n", Duration::from_secs(2));
        assert_eq!(r.outcome, Outcome::Ok);
        assert!(same_tokens(&r.stdout, "5"));

        let spin = compile(
            &tc,
            &sb,
            "spin",
            "int main(){volatile unsigned long long x=0;for(;;)x++;}",
        )
        .unwrap();
        let r = run(&spin, &[], "", Duration::from_millis(200));
        assert_eq!(r.outcome, Outcome::Timeout);

        let bad = compile(&tc, &sb, "bad", "int main(){ return x; }");
        assert!(bad.unwrap_err().contains("error"));
    }
}
