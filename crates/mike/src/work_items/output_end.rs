const KILLED: &str = "[killed]";
const EXITED_OPEN: &str = "[exited with code ";
const EXITED_CLOSE: char = ']';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ended {
    pub state: &'static str,
    pub exit_code: Option<i32>,
}

pub(crate) fn ended(output_tail: &str) -> Option<Ended> {
    let last = output_tail.trim_end().lines().next_back()?.trim();
    if last == KILLED {
        return Some(Ended {
            state: "killed",
            exit_code: None,
        });
    }
    let code: i32 = last
        .strip_prefix(EXITED_OPEN)?
        .strip_suffix(EXITED_CLOSE)?
        .parse()
        .ok()?;
    Some(Ended {
        state: if code == 0 { "completed" } else { "failed" },
        exit_code: Some(code),
    })
}

#[cfg(test)]
mod tests {
    use super::{Ended, ended};

    #[test]
    fn a_job_is_over_only_when_the_last_line_of_its_output_says_so() {
        assert_eq!(
            ended("work\n\n[exited with code 0]\n"),
            Some(Ended {
                state: "completed",
                exit_code: Some(0)
            })
        );
        assert_eq!(
            ended("boom\n[exited with code 101]"),
            Some(Ended {
                state: "failed",
                exit_code: Some(101)
            })
        );
        assert_eq!(
            ended("\n[killed]\n"),
            Some(Ended {
                state: "killed",
                exit_code: None
            })
        );
        assert_eq!(ended(""), None);
        assert_eq!(ended("still going\n"), None);
        assert_eq!(
            ended("[exited with code 0]\nand then more output\n"),
            None,
            "a job that printed another job's last line and went on is still running"
        );
        assert_eq!(ended("[exited with code x]"), None);
    }
}
