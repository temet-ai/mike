use super::*;
use serde_json::json;

use super::super::collect_project_scan_sources;

#[test]
fn extract_project_basic_claude_md_path() {
    let prompt = "Contents of /Users/user/projects/ai/tools/proxy/CLAUDE.md (project instructions):\n\nSome content here.";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/Users/user/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn extract_project_worktree_normalized() {
    let prompt = "Contents of /Users/user/projects/ai/tools/proxy/.claude/worktrees/xyz/CLAUDE.md (project instructions):";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/Users/user/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn extract_project_rules_file() {
    let prompt = "Contents of /Users/user/projects/ai/tools/proxy/.claude/rules/testing.md (project instructions):";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/Users/user/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn extract_project_multiple_projects_picks_shortest() {
    let prompt = "Contents of /Users/user/projects/ai/tools/proxy/CLAUDE.md (project instructions, checked into the codebase):\n\
                  Contents of /Users/user/projects/ai/tools/CLAUDE.md (project instructions, checked into the codebase):";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/Users/user/projects/ai/tools".to_string())
    );
}

#[test]
fn extract_project_no_match_returns_none() {
    let prompt = "This is a normal system prompt without any Contents of lines.";
    assert_eq!(extract_project(sig_from_prompt(prompt)), None);
}

#[test]
fn extract_project_empty_string_returns_none() {
    assert_eq!(extract_project(sig_from_prompt("")), None);
}

#[test]
fn extract_project_user_global_only_returns_none() {
    let prompt = "Contents of /Users/user/.claude/CLAUDE.md (user instructions):";
    assert_eq!(extract_project(sig_from_prompt(prompt)), None);
}

#[test]
fn extract_project_user_global_plus_project_returns_project() {
    let prompt = "Contents of /Users/user/.claude/CLAUDE.md (user instructions):\n\
                   Contents of /Users/user/projects/ai/tools/proxy/CLAUDE.md (project instructions):";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/Users/user/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn extract_project_worktree_rules_path() {
    let prompt = "Contents of /Users/user/projects/ai/tools/proxy/.claude/worktrees/abc/.claude/rules/testing.md (instructions):";
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/Users/user/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn working_directory_marker_in_system_prompt_wins() {
    let sys = "You are Claude Code.\n\
               Working directory: /Users/x/projects/ai/tools/proxy\n\
               More context here.";
    let user = "Contents of /Users/x/deep/nested/somewhere/else/CLAUDE.md (project instructions, checked into the codebase):";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![user],
        }),
        Some("/Users/x/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn working_directory_marker_with_bullet_prefix_is_detected() {
    let sys = "You have been invoked in the following environment:\n\
               \x20- Primary working directory: /Users/x/projects/ai/tools/proxy\n\
               \x20- Is a git repository: true\n\
               \x20- Platform: darwin";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![],
        }),
        Some("/Users/x/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn primary_working_directory_marker_is_equivalent() {
    let sys = "Assistant boilerplate.\n\
               Primary working directory: /Users/x/projects/ai/tools/proxy\n\
               Trailing noise.";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![],
        }),
        Some("/Users/x/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn extract_project_rejects_root_containing_escaped_newline_artifact() {
    let sys = "Working directory: /ai/tools/proxy\\n\\";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![],
        }),
        None
    );
}

#[test]
fn extract_project_rejects_root_containing_unexpanded_template_braces() {
    let sys = "Working directory: /runner/{parent}/{repo}";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![],
        }),
        None
    );
}

#[test]
fn project_instructions_marker_beats_generic_contents_of() {
    let user = "<system-reminder>\n\
                Contents of /Users/x/.claude/CLAUDE.md (user's private global instructions for all projects):\n\
                body 1\n\
                Contents of /Users/x/projects/ai/tools/proxy/CLAUDE.md (project instructions, checked into the codebase):\n\
                body 2\n\
                </system-reminder>";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some("You are Claude Code."),
            message_sources: vec![user],
        }),
        Some("/Users/x/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn falls_through_to_generic_when_no_explicit_markers() {
    let user = "Contents of /Users/x/projects/ai/tools/proxy/CLAUDE.md:\n\
                some content";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: None,
            message_sources: vec![user],
        }),
        Some("/Users/x/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn returns_none_when_no_signals() {
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: None,
            message_sources: vec!["", ""],
        }),
        None
    );
}

#[test]
fn returns_none_when_sources_vec_is_empty() {
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: None,
            message_sources: vec![],
        }),
        None
    );
}

#[test]
fn user_global_claude_md_does_not_count() {
    let user = "<system-reminder>\n\
                Contents of /Users/x/.claude/CLAUDE.md (user's private global instructions for all projects):\n\
                body\n\
                </system-reminder>";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: None,
            message_sources: vec![user],
        }),
        None
    );
}

#[test]
fn worktree_path_normalized_via_working_directory() {
    let sys = "You are Claude Code.\n\
               Working directory: /Users/x/projects/proxy/.claude/worktrees/foo";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![],
        }),
        Some("/Users/x/projects/proxy".to_string())
    );
}

#[test]
fn real_captured_request_yields_projects_ai_tools_proxy() {
    let sys = "x-anthropic-billing-header: cc_version=2.1.111\n\
               ... 60 lines of boilerplate ...\n\
               You are Claude Code, Anthropic's official CLI for Claude.\n\
               ...\n\
               Working directory: /Users/vimes/projects/ai/tools/proxy\n\
               Current branch: ddt/phase-2.8-session-ui";
    let user = "<system-reminder>\n\
                # claudeMd\n\
                Contents of /Users/vimes/projects/ai/tools/proxy/CLAUDE.md (project instructions, checked into the codebase):\n\
                body ...\n\
                </system-reminder>\n\
                Do the thing.";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![user],
        }),
        Some("/Users/vimes/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn extract_project_real_world_system_prompt() {
    let prompt = r#"Contents of /Users/vimes/.claude/CLAUDE.md (user's private global instructions for all projects):

# Global Instructions

Contents of /Users/vimes/projects/ai/tools/CLAUDE.md (project instructions, checked into the codebase):

# ai/tools monorepo

Contents of /Users/vimes/projects/ai/tools/proxy/CLAUDE.md (project instructions, checked into the codebase):

# Claude Proxy — Project Instructions

Contents of /Users/vimes/projects/ai/tools/proxy/.claude/rules/testing.md (project instructions, checked into the codebase):

# Testing Rules"#;
    assert_eq!(
        extract_project(sig_from_prompt(prompt)),
        Some("/Users/vimes/projects/ai/tools".to_string())
    );
}

#[test]
fn working_directory_in_message_body_is_detected() {
    let sys = "You are Claude Code.";
    let user_msg = "<system-reminder>\n\
                    Working directory: /Users/x/projects/ai/tools/proxy\n\
                    </system-reminder>\n\
                    Do the work.";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![user_msg],
        }),
        Some("/Users/x/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn working_directory_in_message_prose_is_ignored() {
    let user_msg = "Here's the brief the agent pasted:\n\
                    Working directory: /prose/only/project\n\
                    please continue from there.";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: None,
            message_sources: vec![user_msg],
        }),
        None
    );
}

#[test]
fn working_directory_in_system_prompt_directly_is_honoured() {
    let sys = "You are Claude Code.\n\
               Working directory: /Users/x/projects/ai/tools/proxy";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![],
        }),
        Some("/Users/x/projects/ai/tools/proxy".to_string())
    );
}

#[test]
fn contents_of_in_tool_result_block_is_detected() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "bash", "input": {}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content":
                    "Contents of /Users/x/projects/foo/CLAUDE.md (project instructions, checked into the codebase):\nbody"
                }
            ]}
        ]
    });
    let sources = collect_project_scan_sources(&body);
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: None,
            message_sources: sources,
        }),
        Some("/Users/x/projects/foo".to_string())
    );
}

#[test]
fn working_directory_still_beats_contents_of_when_in_different_sources() {
    let sys = "You are Claude Code.\n\
               Working directory: /test/project";
    let user_msg = "<system-reminder>\n\
                    Contents of /Users/x/deep/nested/path/to/other/CLAUDE.md (project instructions, checked into the codebase):\n\
                    body\n\
                    </system-reminder>";
    assert_eq!(
        extract_project(ProjectSignals {
            system_prompt: Some(sys),
            message_sources: vec![user_msg],
        }),
        Some("/test/project".to_string())
    );
}
