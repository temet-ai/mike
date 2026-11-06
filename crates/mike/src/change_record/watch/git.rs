use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) const HASH_BATCH: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("could not start git {args} in {repo}: {source}")]
    Spawn {
        repo: String,
        args: String,
        source: std::io::Error,
    },
    #[error("git {args} in {repo} exited with {code:?}: {stderr}")]
    Failed {
        repo: String,
        args: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("git {args} in {repo}: {detail}")]
    Output {
        repo: String,
        args: String,
        detail: String,
    },
}

#[derive(Debug, Clone)]
pub struct Git {
    root: String,
}

impl Git {
    pub fn new(root: impl Into<String>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn absolute(&self, relative: &str) -> PathBuf {
        Path::new(&self.root).join(relative)
    }

    pub async fn run(&self, args: &[&str]) -> Result<Vec<u8>, GitError> {
        self.run_with(args, None, &[0]).await
    }

    pub async fn run_with(
        &self,
        args: &[&str],
        stdin: Option<&[u8]>,
        ok_codes: &[i32],
    ) -> Result<Vec<u8>, GitError> {
        let mut command = tokio::process::Command::new("git");
        command
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|source| GitError::Spawn {
            repo: self.root.clone(),
            args: args.join(" "),
            source,
        })?;
        let input = child.stdin.take();
        let mut out_pipe = child.stdout.take();
        let mut err_pipe = child.stderr.take();
        let feed = async move {
            if let (Some(bytes), Some(mut pipe)) = (stdin, input) {
                pipe.write_all(bytes).await?;
                pipe.shutdown().await?;
            }
            Ok::<(), std::io::Error>(())
        };
        let read_out = drain(&mut out_pipe);
        let read_err = drain(&mut err_pipe);
        let (fed, stdout, stderr) = tokio::join!(feed, read_out, read_err);
        let status = child.wait().await.map_err(|source| GitError::Spawn {
            repo: self.root.clone(),
            args: args.join(" "),
            source,
        })?;
        let code = status.code();
        if fed.is_err() || !code.is_some_and(|c| ok_codes.contains(&c)) {
            return Err(GitError::Failed {
                repo: self.root.clone(),
                args: args.join(" "),
                code,
                stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
            });
        }
        Ok(stdout)
    }

    pub async fn hash_paths(&self, paths: &[String]) -> Result<Vec<(String, String)>, GitError> {
        let mut out = Vec::with_capacity(paths.len());
        for chunk in paths.chunks(HASH_BATCH) {
            match self.hash_chunk(chunk).await {
                Ok(hashed) => out.extend(hashed),
                Err(_) => out.extend(self.hash_one_by_one(chunk).await),
            }
        }
        Ok(out)
    }

    async fn hash_chunk(&self, chunk: &[String]) -> Result<Vec<(String, String)>, GitError> {
        let input = chunk.join("\n") + "\n";
        let raw = self
            .run_with(
                &["hash-object", "-w", "--stdin-paths"],
                Some(input.as_bytes()),
                &[0],
            )
            .await?;
        let text = String::from_utf8_lossy(&raw);
        let shas: Vec<&str> = text.lines().collect();
        if shas.len() != chunk.len() {
            return Err(self.output_error(
                "hash-object -w --stdin-paths",
                format!("{} ids for {} paths", shas.len(), chunk.len()),
            ));
        }
        Ok(chunk
            .iter()
            .cloned()
            .zip(shas.into_iter().map(str::to_string))
            .collect())
    }

    async fn hash_one_by_one(&self, chunk: &[String]) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for path in chunk {
            if let Ok(mut one) = self.hash_chunk(std::slice::from_ref(path)).await {
                out.append(&mut one);
            }
        }
        out
    }

    pub async fn ignored(&self, paths: &[String]) -> Result<Vec<String>, GitError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let input: Vec<u8> = paths
            .iter()
            .flat_map(|p| p.as_bytes().iter().copied().chain(std::iter::once(0)))
            .collect();
        let raw = self
            .run_with(&["check-ignore", "--stdin", "-z"], Some(&input), &[0, 1])
            .await?;
        Ok(split_nul(&raw))
    }

    pub async fn files_under(&self, dir: &str) -> Result<Vec<String>, GitError> {
        let raw = self
            .run(&[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
                "--",
                dir,
            ])
            .await?;
        Ok(split_nul(&raw))
    }

    pub async fn show_toplevel(&self) -> Result<String, GitError> {
        self.one_line(&["rev-parse", "--show-toplevel"]).await
    }

    pub async fn git_dirs(&self) -> Result<(PathBuf, PathBuf), GitError> {
        let raw = self
            .run(&[
                "rev-parse",
                "--path-format=absolute",
                "--git-dir",
                "--git-common-dir",
            ])
            .await?;
        let text = String::from_utf8_lossy(&raw);
        let mut lines = text.lines();
        match (lines.next(), lines.next()) {
            (Some(git_dir), Some(common)) => Ok((PathBuf::from(git_dir), PathBuf::from(common))),
            _ => Err(self.output_error("rev-parse --git-dir --git-common-dir", text.to_string())),
        }
    }

    pub async fn one_line(&self, args: &[&str]) -> Result<String, GitError> {
        let raw = self.run(args).await?;
        let text = String::from_utf8_lossy(&raw).trim().to_string();
        if text.is_empty() {
            return Err(self.output_error(&args.join(" "), "no output".to_string()));
        }
        Ok(text)
    }

    pub fn output_error(&self, args: &str, detail: String) -> GitError {
        GitError::Output {
            repo: self.root.clone(),
            args: args.to_string(),
            detail,
        }
    }
}

async fn drain<R: tokio::io::AsyncRead + Unpin>(pipe: &mut Option<R>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(reader) = pipe.as_mut() {
        let _ = reader.read_to_end(&mut buf).await;
    }
    buf
}

pub fn split_nul(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == 0)
        .filter(|e| !e.is_empty())
        .map(|e| String::from_utf8_lossy(e).into_owned())
        .collect()
}
